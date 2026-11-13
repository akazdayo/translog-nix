use anyhow::{Result, bail};
use nix_narinfo::TrustedPublicKey;
use std::{collections::HashMap, net::SocketAddr, sync::Arc, time::Duration};
use tokio::sync::RwLock;
use translog_common::{base_url, setting};
use translog_proxy::{ProxyState, restore, router, sync_loop};

#[tokio::main]
async fn main() -> Result<()> {
    let keys = setting("TRANSLOG_PUBLIC_KEYS", None)?
        .split_whitespace()
        .map(TrustedPublicKey::parse)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    if keys.is_empty() {
        bail!("at least one public key is required")
    }
    let state = ProxyState {
        upstream: base_url(&setting(
            "TRANSLOG_UPSTREAM",
            Some("https://cache.nixos.org/"),
        )?)?,
        observer: base_url(&setting(
            "TRANSLOG_OBSERVER_URL",
            Some("http://127.0.0.1:8080/"),
        )?)?,
        client: reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()?,
        keys: Arc::new(keys),
        entries: Arc::new(RwLock::new(HashMap::new())),
    };
    let log = base_url(&setting(
        "TRANSLOG_TRANSPARENCY_URL",
        Some("http://127.0.0.1:8082/"),
    )?)?;
    let key = setting("TRANSLOG_LOG_PUBLIC_KEY_FILE", None)?;
    let snapshot = setting("TRANSLOG_SNAPSHOT", Some("translog-snapshot.json"))?;
    let executable = setting("TRANSLOG_SYNC_COMMAND", Some("translog-sync"))?;
    let seconds: u64 = setting("TRANSLOG_SYNC_SECONDS", Some("3600"))?.parse()?;
    if seconds == 0 {
        bail!("sync interval must be positive")
    }
    if std::path::Path::new(&snapshot).exists() {
        if let Err(e) = restore(&state, &executable, &key, &snapshot).await {
            eprintln!("snapshot restore: {e:#}");
        }
    }
    tokio::spawn(sync_loop(
        state.clone(),
        executable,
        log,
        key,
        snapshot,
        Duration::from_secs(seconds),
    ));
    let addr: SocketAddr = setting("TRANSLOG_LISTEN", Some("127.0.0.1:8081"))?.parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    eprintln!("proxy listening on {addr}");
    axum::serve(listener, router(state)).await?;
    Ok(())
}
