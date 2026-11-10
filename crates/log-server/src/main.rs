use anyhow::{Context, Result, anyhow, bail};
use axum::{
    Router,
    extract::{Path, Query, State},
    response::{IntoResponse, Response},
    routing::get,
};
use nix_narinfo::{NarInfo, TrustedPublicKey};
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool, sqlite::SqlitePoolOptions};
use std::{
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use translog_common::{AppResult, base_url, check_hash, setting};
use url::Url;

#[derive(Clone)]
struct LogState {
    db: SqlitePool,
    upstream: Url,
    cache_id: String,
    keys: Arc<Vec<TrustedPublicKey>>,
    client: reqwest::Client,
    errors: Arc<AtomicU64>,
    last_success: Arc<AtomicU64>,
    pending: Arc<AtomicU64>,
}
fn parse_observation(raw: &str, hash: &str, keys: &[TrustedPublicKey]) -> Result<(String, String)> {
    let dir = Default::default();
    let info = NarInfo::parse_in(&dir, raw.as_bytes()).context("invalid narinfo")?;
    let path = info.store_path().to_string();
    let short = path
        .rsplit('/')
        .next()
        .ok_or_else(|| anyhow!("invalid store path"))?;
    if short.split_once('-').map(|(h, _)| h) != Some(hash) {
        bail!("narinfo store path does not match request");
    }
    if !info.has_valid_signature(&dir, keys) {
        bail!("no valid trusted signature");
    }
    Ok((path, info.nar_hash().to_sri_string()))
}
async fn fetch_observation(s: &LogState, hash: &str) -> Result<(String, String, String)> {
    check_hash(hash)?;
    let response = s
        .client
        .get(s.upstream.join(&format!("{hash}.narinfo"))?)
        .send()
        .await?
        .error_for_status()?;
    if response.content_length().is_some_and(|n| n > 1024 * 1024) {
        bail!("narinfo too large");
    }
    let bytes = response.bytes().await?;
    if bytes.len() > 1024 * 1024 {
        bail!("narinfo too large");
    }
    let raw = String::from_utf8(bytes.to_vec())?;
    let (path, nar_hash) = parse_observation(&raw, hash, &s.keys)?;
    Ok((raw, path, nar_hash))
}
async fn observe(s: &LogState, hash: &str) -> Result<()> {
    let (raw, path, nar_hash) = match fetch_observation(s, hash).await {
        Ok(v) => v,
        Err(e) => {
            s.errors.fetch_add(1, Ordering::Relaxed);
            return Err(e);
        }
    };
    let result: Result<()> = async {
        let mut tx = s.db.begin().await?;
        // The first successful writer establishes the immutable baseline.
        sqlx::query("INSERT INTO observations (cache_id, endpoint, store_hash, store_path, nar_hash, narinfo, observed_at) VALUES (?, ?, ?, ?, ?, ?, unixepoch())")
            .bind(&s.cache_id).bind(s.upstream.as_str()).bind(hash).bind(&path).bind(&nar_hash).bind(&raw).execute(&mut *tx).await?;
        let id: i64 = sqlx::query_scalar("SELECT last_insert_rowid()").fetch_one(&mut *tx).await?;
        sqlx::query("INSERT OR IGNORE INTO baselines (cache_id, store_hash, observation_id) VALUES (?, ?, ?)")
            .bind(&s.cache_id).bind(hash).bind(id).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }.await;
    if result.is_err() {
        s.errors.fetch_add(1, Ordering::Relaxed);
    }
    result?;
    s.last_success.store(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs(),
        Ordering::Relaxed,
    );
    Ok(())
}
async fn baseline(s: &LogState, hash: &str) -> Result<Option<String>> {
    Ok(sqlx::query_scalar("SELECT o.narinfo FROM baselines b JOIN observations o ON o.id = b.observation_id WHERE b.cache_id = ? AND b.store_hash = ?")
        .bind(&s.cache_id).bind(hash).fetch_optional(&s.db).await?)
}
async fn log_narinfo(State(s): State<LogState>, Path(file): Path<String>) -> AppResult<Response> {
    let hash = file
        .strip_suffix(".narinfo")
        .ok_or_else(|| anyhow!("invalid request"))?;
    check_hash(hash)?;
    let raw = match baseline(&s, hash).await? {
        Some(raw) => raw,
        None => {
            observe(&s, hash).await?;
            baseline(&s, hash)
                .await?
                .ok_or_else(|| anyhow!("baseline missing"))?
        }
    };
    Ok(([("content-type", "text/x-nix-narinfo")], raw).into_response())
}
#[derive(Serialize)]
struct Record {
    id: i64,
    cache_id: String,
    endpoint: String,
    store_path: String,
    nar_hash: String,
    narinfo: String,
    observed_at: i64,
    baseline: bool,
}
#[derive(Deserialize, Default)]
struct HistoryQuery {
    after: Option<i64>,
}
async fn history(
    State(s): State<LogState>,
    Query(q): Query<HistoryQuery>,
) -> AppResult<axum::Json<Vec<Record>>> {
    let rows = sqlx::query("SELECT o.*, EXISTS(SELECT 1 FROM baselines b WHERE b.observation_id = o.id) AS baseline FROM observations o WHERE o.id > ? ORDER BY o.id LIMIT 1000").bind(q.after.unwrap_or(0).max(0)).fetch_all(&s.db).await?;
    Ok(axum::Json(
        rows.into_iter()
            .map(|r| Record {
                id: r.get("id"),
                cache_id: r.get("cache_id"),
                endpoint: r.get("endpoint"),
                store_path: r.get("store_path"),
                nar_hash: r.get("nar_hash"),
                narinfo: r.get("narinfo"),
                observed_at: r.get("observed_at"),
                baseline: r.get::<i64, _>("baseline") != 0,
            })
            .collect(),
    ))
}
async fn metrics(State(s): State<LogState>) -> AppResult<String> {
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM baselines WHERE cache_id = ?")
        .bind(&s.cache_id)
        .fetch_one(&s.db)
        .await?;
    let mismatch: i64 = sqlx::query_scalar("SELECT count(*) FROM observations o JOIN baselines b ON b.cache_id=o.cache_id AND b.store_hash=o.store_hash JOIN observations base ON base.id=b.observation_id WHERE o.cache_id=? AND o.nar_hash != base.nar_hash").bind(&s.cache_id).fetch_one(&s.db).await?;
    Ok(format!(
        "# TYPE translog_baselines gauge\ntranslog_baselines {count}\n# TYPE translog_nar_hash_mismatches_total counter\ntranslog_nar_hash_mismatches_total {mismatch}\n# TYPE translog_observation_errors_total counter\ntranslog_observation_errors_total {}\n# TYPE translog_last_successful_observation_timestamp_seconds gauge\ntranslog_last_successful_observation_timestamp_seconds {}\n# TYPE translog_reobservation_pending gauge\ntranslog_reobservation_pending {}\n",
        s.errors.load(Ordering::Relaxed),
        s.last_success.load(Ordering::Relaxed),
        s.pending.load(Ordering::Relaxed)
    ))
}
async fn log_proxy_narinfo(State(s): State<LogState>, path: Path<String>) -> AppResult<Response> {
    let hash = path
        .0
        .strip_suffix(".narinfo")
        .ok_or_else(|| anyhow!("invalid request"))?
        .to_owned();
    let mut resp = log_narinfo(State(s.clone()), path).await?;
    let endpoint: String = sqlx::query_scalar("SELECT o.endpoint FROM baselines b JOIN observations o ON o.id=b.observation_id WHERE b.cache_id=? AND b.store_hash=?")
        .bind(&s.cache_id).bind(hash).fetch_one(&s.db).await?;
    resp.headers_mut()
        .insert("x-translog-upstream", endpoint.parse()?);
    Ok(resp)
}
#[tokio::main]
async fn main() -> Result<()> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;
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
    let db_path = setting("TRANSLOG_DATABASE", Some("translog.sqlite"))?;
    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(db_path)
        .create_if_missing(true)
        .busy_timeout(Duration::from_secs(10))
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal);
    let db = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;
    sqlx::query("CREATE TABLE IF NOT EXISTS observations (id INTEGER PRIMARY KEY AUTOINCREMENT, cache_id TEXT NOT NULL, endpoint TEXT NOT NULL, store_hash TEXT NOT NULL, store_path TEXT NOT NULL, nar_hash TEXT NOT NULL, narinfo TEXT NOT NULL, observed_at INTEGER NOT NULL)").execute(&db).await?;
    sqlx::query("CREATE TABLE IF NOT EXISTS baselines (cache_id TEXT NOT NULL, store_hash TEXT NOT NULL, observation_id INTEGER NOT NULL REFERENCES observations(id), PRIMARY KEY(cache_id, store_hash))").execute(&db).await?;
    let s = LogState {
        db,
        cache_id: setting("TRANSLOG_CACHE_ID", Some(upstream.as_str()))?,
        upstream,
        keys: Arc::new(keys),
        client,
        errors: Arc::new(AtomicU64::new(0)),
        last_success: Arc::new(AtomicU64::new(0)),
        pending: Arc::new(AtomicU64::new(0)),
    };
    let seconds: u64 = setting("TRANSLOG_REOBSERVE_SECONDS", Some("3600"))?.parse()?;
    if seconds == 0 {
        bail!("reobservation interval must be positive");
    }
    let bg = s.clone();
    tokio::spawn(async move {
        let mut timer = tokio::time::interval(Duration::from_secs(seconds));
        timer.tick().await;
        loop {
            timer.tick().await;
            match sqlx::query_scalar::<_, String>(
                "SELECT store_hash FROM baselines WHERE cache_id = ? ORDER BY store_hash",
            )
            .bind(&bg.cache_id)
            .fetch_all(&bg.db)
            .await
            {
                Ok(hashes) => {
                    bg.pending.store(hashes.len() as u64, Ordering::Relaxed);
                    for hash in hashes {
                        if let Err(e) = observe(&bg, &hash).await {
                            eprintln!("reobservation {hash}: {e:#}");
                        }
                        bg.pending.fetch_sub(1, Ordering::Relaxed);
                    }
                }
                Err(e) => {
                    bg.errors.fetch_add(1, Ordering::Relaxed);
                    eprintln!("reobservation query: {e}");
                }
            }
        }
    });
    let app = Router::new()
        .route("/metrics", get(metrics))
        .route("/observations", get(history))
        .route("/{file}", get(log_proxy_narinfo))
        .with_state(s);
    let addr: SocketAddr = setting("TRANSLOG_LISTEN", Some("127.0.0.1:8080"))?.parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    eprintln!("log listening on {addr}");
    axum::serve(listener, app).await?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use base64::{Engine, engine::general_purpose::STANDARD};
    use ed25519_dalek::{Signer, SigningKey};
    use tokio::sync::RwLock;

    const HASH: &str = "ff43q3pvpl1sdyxj88dy8a0wzg47bih2";
    fn signed(hash: &str) -> (String, TrustedPublicKey) {
        let key = SigningKey::from_bytes(&[7; 32]);
        let raw = format!(
            "StorePath: /nix/store/{HASH}-example\nURL: nar/example.nar\nCompression: none\nNarHash: {hash}\nNarSize: 144\nReferences: \n"
        );
        let info = NarInfo::parse_in(&Default::default(), raw.as_bytes()).unwrap();
        let signature = key.sign(info.fingerprint(&Default::default()).as_bytes());
        let public = TrustedPublicKey::parse(&format!(
            "test:{}",
            STANDARD.encode(key.verifying_key().as_bytes())
        ))
        .unwrap();
        (
            format!("{raw}Sig: test:{}\n", STANDARD.encode(signature.to_bytes())),
            public,
        )
    }
    #[tokio::test]
    async fn observation_freezes_first_signed_value_and_records_changes() {
        let (first, key) = signed("sha256:1fm3wgw7wsx66fhyzi90l1avyn89qpa4fkz7064sb61j5xy8kx65");
        let (second, _) = signed("sha256:0fm3wgw7wsx66fhyzi90l1avyn89qpa4fkz7064sb61j5xy8kx65");
        let content = Arc::new(RwLock::new(first.clone()));
        let app = Router::new().route(
            "/{file}",
            get({
                let content = content.clone();
                move || {
                    let content = content.clone();
                    async move { content.read().await.clone() }
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let db = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query("CREATE TABLE observations (id INTEGER PRIMARY KEY AUTOINCREMENT, cache_id TEXT NOT NULL, endpoint TEXT NOT NULL, store_hash TEXT NOT NULL, store_path TEXT NOT NULL, nar_hash TEXT NOT NULL, narinfo TEXT NOT NULL, observed_at INTEGER NOT NULL)").execute(&db).await.unwrap();
        sqlx::query("CREATE TABLE baselines (cache_id TEXT NOT NULL, store_hash TEXT NOT NULL, observation_id INTEGER NOT NULL, PRIMARY KEY(cache_id, store_hash))").execute(&db).await.unwrap();
        let state = LogState {
            db,
            upstream: Url::parse(&format!("http://{addr}/")).unwrap(),
            cache_id: "test".into(),
            keys: Arc::new(vec![key]),
            client: reqwest::Client::new(),
            errors: Arc::new(AtomicU64::new(0)),
            last_success: Arc::new(AtomicU64::new(0)),
            pending: Arc::new(AtomicU64::new(0)),
        };
        let log_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let log_addr = log_listener.local_addr().unwrap();
        let log_app = Router::new()
            .route("/{file}", get(log_proxy_narinfo))
            .with_state(state.clone());
        let log_server =
            tokio::spawn(async move { axum::serve(log_listener, log_app).await.unwrap() });
        let proxy_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = proxy_listener.local_addr().unwrap();
        let proxy_app = translog_proxy::router(
            Url::parse(&format!("http://{log_addr}/")).unwrap(),
            reqwest::Client::new(),
        );
        let proxy_server =
            tokio::spawn(async move { axum::serve(proxy_listener, proxy_app).await.unwrap() });
        let client = reqwest::Client::new();
        let url = format!("http://{proxy_addr}/{HASH}.narinfo");
        let served = client
            .get(&url)
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .text()
            .await
            .unwrap();
        assert!(served.contains(&format!("URL: http://{addr}/nar/example.nar")));
        assert!(served.contains("Sig: test:"));
        *content.write().await = second.clone();
        observe(&state, HASH).await.unwrap();
        assert_eq!(baseline(&state, HASH).await.unwrap().unwrap(), first);
        let served_after = client
            .get(&url)
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .text()
            .await
            .unwrap();
        assert_eq!(served, served_after);
        assert!(
            metrics(State(state.clone()))
                .await
                .unwrap()
                .contains("translog_nar_hash_mismatches_total 1")
        );
        *content.write().await = second.replace("Sig: test:", "Sig: wrong:");
        assert!(observe(&state, HASH).await.is_err());
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM observations")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(count, 2);
        assert_eq!(state.errors.load(Ordering::Relaxed), 1);
        server.abort();
        log_server.abort();
        proxy_server.abort();
    }
    #[test]
    fn rejects_wrong_path_or_signature() {
        let (raw, key) = signed("sha256:1fm3wgw7wsx66fhyzi90l1avyn89qpa4fkz7064sb61j5xy8kx65");
        assert!(parse_observation(&raw, HASH, &[key.clone()]).is_ok());
        assert!(
            parse_observation(&raw, "0f43q3pvpl1sdyxj88dy8a0wzg47bih2", &[key.clone()]).is_err()
        );
        assert!(
            parse_observation(&raw.replace("NarSize: 144", "NarSize: 145"), HASH, &[key]).is_err()
        );
    }
}
