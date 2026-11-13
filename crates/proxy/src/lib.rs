use anyhow::{Context, Result, anyhow, bail};
use axum::{
    Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use nix_narinfo::{NarInfo, TrustedPublicKey};
use serde::Deserialize;
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::sync::RwLock;
use translog_common::{AppResult, check_hash};
use url::Url;

#[derive(Clone)]
pub struct ProxyState {
    pub upstream: Url,
    pub observer: Url,
    pub client: reqwest::Client,
    pub keys: Arc<Vec<TrustedPublicKey>>,
    pub entries: Arc<RwLock<HashMap<String, String>>>,
}
#[derive(Deserialize)]
struct Snapshot {
    entries: Vec<String>,
}
#[derive(Deserialize)]
struct Observation {
    endpoint: String,
    store_path: String,
    nar_hash: String,
    narinfo: String,
}

// Log entries are untrusted even after their inclusion is verified: validate Nix signatures.
fn index(
    snapshot: &[u8],
    upstream: &Url,
    keys: &[TrustedPublicKey],
) -> Result<HashMap<String, String>> {
    let snapshot: Snapshot = serde_json::from_slice(snapshot)?;
    let mut result = HashMap::new();
    for encoded in snapshot.entries {
        let bytes = STANDARD.decode(encoded)?;
        let Ok(entry) = serde_json::from_slice::<Observation>(&bytes) else {
            continue;
        };
        if entry.endpoint != upstream.as_str() {
            continue;
        }
        let dir = Default::default();
        let Ok(info) = NarInfo::parse_in(&dir, entry.narinfo.as_bytes()) else {
            continue;
        };
        let path = info.store_path().to_string();
        if path != entry.store_path
            || info.nar_hash().to_sri_string() != entry.nar_hash
            || !info.has_valid_signature(&dir, keys)
        {
            continue;
        }
        let Some((hash, _)) = path.rsplit('/').next().and_then(|s| s.split_once('-')) else {
            continue;
        };
        if check_hash(hash).is_err() {
            continue;
        }
        result.entry(hash.to_owned()).or_insert(entry.narinfo);
    }
    Ok(result)
}

pub async fn restore(
    state: &ProxyState,
    executable: &str,
    key_file: &str,
    snapshot: &str,
) -> Result<()> {
    let status = tokio::process::Command::new(executable)
        .arg("-offline")
        .arg("-public-key")
        .arg(key_file)
        .arg("-output")
        .arg(snapshot)
        .status()
        .await?;
    if !status.success() {
        bail!("saved snapshot verification failed")
    }
    let bytes = tokio::fs::read(snapshot).await?;
    *state.entries.write().await = index(&bytes, &state.upstream, &state.keys)?;
    Ok(())
}
pub async fn sync_loop(
    state: ProxyState,
    executable: String,
    log: Url,
    key_file: String,
    snapshot: String,
    interval: Duration,
) {
    loop {
        let result: Result<()> = async {
            let status = tokio::process::Command::new(&executable)
                .arg("-url")
                .arg(log.as_str())
                .arg("-public-key")
                .arg(&key_file)
                .arg("-output")
                .arg(&snapshot)
                .status()
                .await?;
            if !status.success() {
                bail!("log sync failed: {status}")
            }
            let bytes = tokio::fs::read(&snapshot).await?;
            let updated = index(&bytes, &state.upstream, &state.keys)?;
            *state.entries.write().await = updated;
            Ok(())
        }
        .await;
        if let Err(e) = result {
            eprintln!("log sync: {e:#}");
        }
        tokio::time::sleep(interval).await;
    }
}
pub fn router(state: ProxyState) -> Router {
    Router::new()
        .route(
            "/nix-cache-info",
            get(|| async { "StoreDir: /nix/store\nWantMassQuery: 0\nPriority: 40\n" }),
        )
        .route("/{file}", get(proxy_narinfo))
        .with_state(state)
}
async fn proxy_narinfo(
    State(s): State<ProxyState>,
    Path(file): Path<String>,
) -> AppResult<Response> {
    let hash = file
        .strip_suffix(".narinfo")
        .ok_or_else(|| anyhow!("invalid request"))?;
    check_hash(hash)?;
    let saved = s.entries.read().await.get(hash).cloned();
    let raw = match saved {
        Some(raw) => {
            eprintln!("narinfo {hash}: serving verified log entry");
            raw
        }
        None => {
            eprintln!("narinfo {hash}: fetching from upstream and requesting observation");
            let response = s.client.get(s.upstream.join(&file)?).send().await?;
            if response.status() == StatusCode::NOT_FOUND {
                return Ok(StatusCode::NOT_FOUND.into_response());
            }
            let response = response.error_for_status()?;
            if response.content_length().is_some_and(|n| n > 1024 * 1024) {
                return Err(anyhow!("narinfo too large").into());
            }
            let bytes = response.bytes().await?;
            if bytes.len() > 1024 * 1024 {
                return Err(anyhow!("narinfo too large").into());
            }
            let raw = String::from_utf8(bytes.to_vec())?;
            let dir = Default::default();
            let info = NarInfo::parse_in(&dir, raw.as_bytes()).context("invalid narinfo")?;
            if info
                .store_path()
                .to_string()
                .rsplit('/')
                .next()
                .and_then(|p| p.split_once('-'))
                .map(|(h, _)| h)
                != Some(hash)
                || !info.has_valid_signature(&dir, &s.keys)
            {
                return Err(anyhow!("invalid path or signature").into());
            }
            let client = s.client.clone();
            let url = s.observer.join(&format!("observe/{hash}"))?;
            tokio::spawn(async move {
                if let Err(e) = async {
                    client.post(url).send().await?.error_for_status()?;
                    Result::<()>::Ok(())
                }
                .await
                {
                    eprintln!("observe: {e:#}");
                }
            });
            raw
        }
    };
    let info = NarInfo::parse_in(&Default::default(), raw.as_bytes())?;
    let nar_url = s.upstream.join(info.url())?;
    if !matches!(nar_url.scheme(), "http" | "https") {
        return Err(anyhow!("unsupported NAR URL").into());
    }
    let rewritten = raw
        .lines()
        .map(|line| {
            if line.starts_with("URL: ") {
                format!("URL: {nar_url}\n")
            } else {
                format!("{line}\n")
            }
        })
        .collect::<String>();
    Ok(([("content-type", "text/x-nix-narinfo")], rewritten).into_response())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    const HASH: &str = "ff43q3pvpl1sdyxj88dy8a0wzg47bih2";
    fn signed(nar_hash: &str) -> (String, TrustedPublicKey) {
        let key = SigningKey::from_bytes(&[7; 32]);
        let raw = format!(
            "StorePath: /nix/store/{HASH}-example\nURL: nar/example.nar\nCompression: none\nNarHash: {nar_hash}\nNarSize: 144\nReferences: \n"
        );
        let info = NarInfo::parse_in(&Default::default(), raw.as_bytes()).unwrap();
        let sig = key.sign(info.fingerprint(&Default::default()).as_bytes());
        let public = TrustedPublicKey::parse(&format!(
            "test:{}",
            STANDARD.encode(key.verifying_key().as_bytes())
        ))
        .unwrap();
        (
            format!("{raw}Sig: test:{}\n", STANDARD.encode(sig.to_bytes())),
            public,
        )
    }
    #[test]
    fn indexes_first_valid_entry_for_upstream() {
        let (first, key) = signed("sha256:1fm3wgw7wsx66fhyzi90l1avyn89qpa4fkz7064sb61j5xy8kx65");
        let (second, _) = signed("sha256:0fm3wgw7wsx66fhyzi90l1avyn89qpa4fkz7064sb61j5xy8kx65");
        let make = |raw: &str, endpoint: &str| {
            let info = NarInfo::parse_in(&Default::default(), raw.as_bytes()).unwrap();
            STANDARD.encode(serde_json::to_vec(&serde_json::json!({"endpoint":endpoint,"store_path":info.store_path().to_string(),"nar_hash":info.nar_hash().to_sri_string(),"narinfo":raw})).unwrap())
        };
        let upstream = Url::parse("https://example.org/").unwrap();
        let snapshot=serde_json::to_vec(&serde_json::json!({"entries":[make(&second,"https://other/"),make(&first.replace("Sig: test:","Sig: wrong:"),upstream.as_str()),make(&first,upstream.as_str()),make(&second,upstream.as_str())]})).unwrap();
        assert_eq!(
            index(&snapshot, &upstream, &[key]).unwrap().get(HASH),
            Some(&first)
        );
    }
    #[tokio::test]
    async fn fetches_upstream_notifies_observer_and_then_serves_local_baseline() {
        use axum::routing::post;
        let (first, key) = signed("sha256:1fm3wgw7wsx66fhyzi90l1avyn89qpa4fkz7064sb61j5xy8kx65");
        let (baseline, _) = signed("sha256:0fm3wgw7wsx66fhyzi90l1avyn89qpa4fkz7064sb61j5xy8kx65");
        let notified = Arc::new(tokio::sync::Notify::new());
        let app = Router::new()
            .route(
                "/observe/{hash}",
                post({
                    let notified = notified.clone();
                    move || {
                        let notified = notified.clone();
                        async move {
                            notified.notify_one();
                            StatusCode::NO_CONTENT
                        }
                    }
                }),
            )
            .route(
                "/{file}",
                get({
                    let first = first.clone();
                    move || {
                        let first = first.clone();
                        async move { first }
                    }
                }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = Url::parse(&format!("http://{}/", listener.local_addr().unwrap())).unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let state = ProxyState {
            upstream: url.clone(),
            observer: url,
            client: reqwest::Client::new(),
            keys: Arc::new(vec![key]),
            entries: Arc::new(RwLock::new(HashMap::new())),
        };
        let res = proxy_narinfo(State(state.clone()), Path(format!("{HASH}.narinfo")))
            .await
            .unwrap();
        let body = axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap();
        assert!(
            String::from_utf8(body.to_vec())
                .unwrap()
                .contains("1fm3wgw7wsx66fhyzi90l1avyn89qpa4fkz7064sb61j5xy8kx65")
        );
        tokio::time::timeout(Duration::from_secs(2), notified.notified())
            .await
            .unwrap();
        state.entries.write().await.insert(HASH.into(), baseline);
        let res = proxy_narinfo(State(state), Path(format!("{HASH}.narinfo")))
            .await
            .unwrap();
        let body = axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap();
        assert!(
            String::from_utf8(body.to_vec())
                .unwrap()
                .contains("0fm3wgw7wsx66fhyzi90l1avyn89qpa4fkz7064sb61j5xy8kx65")
        );
        server.abort();
    }
}
