use crate::errors::AppError;
use axum::http::HeaderMap;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug)]
pub struct AuthUser {
    pub id: i64,
    pub email: String,
}

#[derive(Debug)]
pub struct AuthDevice {
    pub id: i64,
    pub name: String,
}

pub fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is after 1970")
        .as_secs() as i64
}

pub fn extract_bearer_token(headers: &HeaderMap) -> Result<String, AppError> {
    let header = headers.get("Authorization").ok_or(AppError::Unauthorized)?;

    let header = header.to_str().map_err(|_| AppError::Unauthorized)?;

    let token = header
        .strip_prefix("Bearer ")
        .ok_or(AppError::Unauthorized)?;

    Ok(token.to_string())
}
