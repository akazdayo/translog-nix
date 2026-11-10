use anyhow::anyhow;
use axum::{
    Router,
    extract::{Path, State},
    response::{IntoResponse, Response},
    routing::get,
};
use nix_narinfo::NarInfo;
use translog_common::{AppResult, check_hash};
use url::Url;

#[derive(Clone)]
struct ProxyState {
    log: Url,
    client: reqwest::Client,
}

pub fn router(log: Url, client: reqwest::Client) -> Router {
    Router::new()
        .route(
            "/nix-cache-info",
            get(|| async { "StoreDir: /nix/store\nWantMassQuery: 0\nPriority: 40\n" }),
        )
        .route("/{file}", get(proxy_narinfo))
        .with_state(ProxyState { log, client })
}

async fn proxy_narinfo(
    State(s): State<ProxyState>,
    Path(file): Path<String>,
) -> AppResult<Response> {
    let hash = file
        .strip_suffix(".narinfo")
        .ok_or_else(|| anyhow!("invalid request"))?;
    check_hash(hash)?;
    let response = s
        .client
        .get(s.log.join(&file)?)
        .send()
        .await?
        .error_for_status()?;
    let upstream = response
        .headers()
        .get("x-translog-upstream")
        .ok_or_else(|| anyhow!("missing upstream header"))?
        .to_str()?
        .to_owned();
    let raw = response.text().await?;
    let info = NarInfo::parse_in(&Default::default(), raw.as_bytes())?;
    let base = Url::parse(&upstream)?;
    let nar_url = base.join(info.url())?;
    if !matches!(nar_url.scheme(), "http" | "https") {
        return Err(anyhow!("unsupported NAR URL").into());
    }
    // URL is not covered by the Nix signature; retain all other original fields.
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
