use anyhow::Result;
use std::{net::SocketAddr, time::Duration};
use translog_common::{base_url, setting};

#[tokio::main]
async fn main() -> Result<()> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;
    let log = base_url(&setting(
        "TRANSLOG_LOG_URL",
        Some("http://127.0.0.1:8080/"),
    )?)?;
    let addr: SocketAddr = setting("TRANSLOG_LISTEN", Some("127.0.0.1:8081"))?.parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    eprintln!("proxy listening on {addr}");
    axum::serve(listener, translog_proxy::router(log, client)).await?;
    Ok(())
}
