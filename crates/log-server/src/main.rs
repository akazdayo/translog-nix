use anyhow::{Context, Result, anyhow, bail};
use axum::{
    Router,
    extract::{Path, State},
    http::StatusCode,
    routing::post,
};
use nix_narinfo::{NarInfo, TrustedPublicKey};
use serde::Serialize;
use std::{net::SocketAddr, sync::Arc, time::Duration};
use translog_common::{AppResult, base_url, check_hash, setting};
use url::Url;

#[derive(Clone)]
struct Observer {
    upstream: Url,
    transparency: Url,
    keys: Arc<Vec<TrustedPublicKey>>,
    client: reqwest::Client,
}
#[derive(Serialize)]
struct Observation<'a> {
    endpoint: &'a str,
    store_path: String,
    nar_hash: String,
    narinfo: &'a str,
    observed_at: u64,
}
async fn observe(State(s): State<Observer>, Path(hash): Path<String>) -> AppResult<StatusCode> {
    check_hash(&hash)?;
    let response = s
        .client
        .get(s.upstream.join(&format!("{hash}.narinfo"))?)
        .send()
        .await?;
    if response.status() == StatusCode::NOT_FOUND {
        return Ok(StatusCode::NOT_FOUND);
    }
    let response = response.error_for_status()?;
    if response.content_length().is_some_and(|n| n > 1024 * 1024) {
        return Err(anyhow!("narinfo too large").into());
    };
    let bytes = response.bytes().await?;
    if bytes.len() > 1024 * 1024 {
        return Err(anyhow!("narinfo too large").into());
    };
    let raw = std::str::from_utf8(&bytes)?;
    let dir = Default::default();
    let info = NarInfo::parse_in(&dir, raw.as_bytes()).context("invalid narinfo")?;
    let path = info.store_path().to_string();
    if path
        .rsplit('/')
        .next()
        .and_then(|s| s.split_once('-'))
        .map(|(h, _)| h)
        != Some(hash.as_str())
    {
        return Err(anyhow!("narinfo store path does not match request").into());
    }
    if !info.has_valid_signature(&dir, &s.keys) {
        return Err(anyhow!("no valid trusted signature").into());
    }
    let entry = Observation {
        endpoint: s.upstream.as_str(),
        store_path: path,
        nar_hash: info.nar_hash().to_sri_string(),
        narinfo: raw,
        observed_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs(),
    };
    s.client
        .post(s.transparency.clone())
        .header("content-type", "application/json")
        .body(serde_json::to_vec(&entry)?)
        .send()
        .await?
        .error_for_status()?;
    eprintln!("observed {hash}: appended to transparency log");
    Ok(StatusCode::NO_CONTENT)
}
#[tokio::main]
async fn main() -> Result<()> {
    let upstream = base_url(&setting(
        "TRANSLOG_UPSTREAM",
        Some("https://cache.nixos.org/"),
    )?)?;
    let keys = setting("TRANSLOG_PUBLIC_KEYS", None)?
        .split_whitespace()
        .map(TrustedPublicKey::parse)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    if keys.is_empty() {
        bail!("at least one public key is required");
    }
    let s = Observer {
        upstream,
        transparency: base_url(&setting(
            "TRANSLOG_TRANSPARENCY_URL",
            Some("http://127.0.0.1:8082/"),
        )?)?
        .join("add")?,
        keys: Arc::new(keys),
        client: reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()?,
    };
    let addr: SocketAddr = setting("TRANSLOG_LISTEN", Some("127.0.0.1:8080"))?.parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    eprintln!("observer listening on {addr}");
    axum::serve(
        listener,
        Router::new()
            .route("/observe/{hash}", post(observe))
            .with_state(s),
    )
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{Engine, engine::general_purpose::STANDARD};
    use ed25519_dalek::{Signer, SigningKey};
    use tokio::sync::Mutex;
    const HASH: &str = "ff43q3pvpl1sdyxj88dy8a0wzg47bih2";
    #[tokio::test]
    async fn observes_valid_narinfo_and_rejects_invalid_signature() {
        let key = SigningKey::from_bytes(&[7; 32]);
        let raw = format!(
            "StorePath: /nix/store/{HASH}-example\nURL: nar/example.nar\nCompression: none\nNarHash: sha256:1fm3wgw7wsx66fhyzi90l1avyn89qpa4fkz7064sb61j5xy8kx65\nNarSize: 144\nReferences: \n"
        );
        let info = NarInfo::parse_in(&Default::default(), raw.as_bytes()).unwrap();
        let signature = key.sign(info.fingerprint(&Default::default()).as_bytes());
        let raw = format!("{raw}Sig: test:{}\n", STANDARD.encode(signature.to_bytes()));
        let public = TrustedPublicKey::parse(&format!(
            "test:{}",
            STANDARD.encode(key.verifying_key().as_bytes())
        ))
        .unwrap();
        let received = Arc::new(Mutex::new(Vec::<String>::new()));
        let app = Router::new()
            .route(
                "/add",
                post({
                    let received = received.clone();
                    move |body: String| {
                        let received = received.clone();
                        async move {
                            received.lock().await.push(body);
                            StatusCode::OK
                        }
                    }
                }),
            )
            .route(
                "/{file}",
                axum::routing::get({
                    let raw = raw.clone();
                    move || {
                        let raw = raw.clone();
                        async move { raw }
                    }
                }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = Url::parse(&format!("http://{}/", listener.local_addr().unwrap())).unwrap();
        let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let mut state = Observer {
            upstream: url.clone(),
            transparency: url.join("add").unwrap(),
            keys: Arc::new(vec![public]),
            client: reqwest::Client::new(),
        };
        assert_eq!(
            observe(State(state.clone()), Path(HASH.into()))
                .await
                .unwrap(),
            StatusCode::NO_CONTENT
        );
        let entries = received.lock().await;
        assert_eq!(entries.len(), 1);
        let entry: serde_json::Value = serde_json::from_str(&entries[0]).unwrap();
        assert_eq!(entry["narinfo"], raw);
        drop(entries);
        state.keys = Arc::new(vec![]);
        assert!(observe(State(state), Path(HASH.into())).await.is_err());
        assert_eq!(received.lock().await.len(), 1);
        task.abort();
    }
}
