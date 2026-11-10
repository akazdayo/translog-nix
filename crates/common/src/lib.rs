use anyhow::{Result, anyhow, bail};
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use std::env;
use url::Url;

#[derive(Debug)]
pub struct AppError(anyhow::Error);
impl<E: Into<anyhow::Error>> From<E> for AppError {
    fn from(e: E) -> Self {
        Self(e.into())
    }
}
impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        eprintln!("request failed: {:#}", self.0);
        (StatusCode::BAD_GATEWAY, "upstream or log error\n").into_response()
    }
}
pub type AppResult<T> = std::result::Result<T, AppError>;

pub fn check_hash(hash: &str) -> Result<()> {
    if hash.len() != 32
        || !hash
            .bytes()
            .all(|c| b"0123456789abcdfghijklmnpqrsvwxyz".contains(&c))
    {
        bail!("invalid store path hash");
    }
    Ok(())
}
pub fn setting(name: &str, default: Option<&str>) -> Result<String> {
    env::var(name)
        .ok()
        .or_else(|| default.map(str::to_owned))
        .ok_or_else(|| anyhow!("{name} is required"))
}
pub fn base_url(value: &str) -> Result<Url> {
    let u = Url::parse(value)?;
    if !matches!(u.scheme(), "http" | "https")
        || !u.path().ends_with('/')
        || u.query().is_some()
        || u.fragment().is_some()
    {
        bail!("URL must be http(s), end in /, and have no query or fragment");
    }
    Ok(u)
}
