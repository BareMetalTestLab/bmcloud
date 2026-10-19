use crate::auth::{AuthDevice, AuthUser, extract_bearer_token, unix_now};
use crate::errors::AppError;
use crate::state::AppState;

use axum::{
    Json,
    extract::{FromRequestParts, Path, State},
    http::request::Parts,
    http::{HeaderMap, StatusCode},
};

use rusqlite::params;

use serde::{Deserialize, Serialize};

use uuid::Uuid;

const DEVICE_ONLINE_TIMEOUT_SECS: i64 = 30;

#[derive(Debug, Deserialize)]
pub struct CreateDeviceRequest {
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct CreateDeviceResponse {
    device: DeviceResponse,
    token: String,
}

#[derive(Debug, Deserialize)]
pub struct DeviceStateRequest {
    state: String,
}

#[derive(Debug, Serialize)]
pub struct DeviceResponse {
    id: i64,
    name: String,
    state: String,
    updated_at: i64,
}

impl FromRequestParts<AppState> for AuthDevice {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let device = current_device(state, &parts.headers)?;
        Ok(device)
    }
}

fn current_device(state: &AppState, headers: &HeaderMap) -> Result<AuthDevice, AppError> {
    let db = state.db.lock().unwrap();

    let token = extract_bearer_token(headers)?;

    match db.query_row(
        "SELECT id, name FROM devices WHERE token = ?1",
        params![token],
        |row| {
            Ok(AuthDevice {
                id: row.get(0)?,
                name: row.get(1)?,
            })
        },
    ) {
        Ok(device) => return Ok(device),
        Err(rusqlite::Error::QueryReturnedNoRows) => Err(AppError::Unauthorized),
        Err(e) => return Err(e.into()),
    }
}

pub async fn device_create(
    State(state): State<AppState>,
    user: AuthUser,
    Json(payload): Json<CreateDeviceRequest>,
) -> Result<(StatusCode, Json<CreateDeviceResponse>), AppError> {
    let name = payload.name.trim().to_string();
    if name.is_empty() {
        return Err(AppError::InvalidDeviceName);
    }

    let token = Uuid::new_v4().to_string();

    let db = state.db.lock().unwrap();
    let now = unix_now();

    db.execute(
        "INSERT INTO devices (user_id, name, token, created_at) VALUES (?1, ?2, ?3, ?4)",
        params![user.id, name, token, now],
    )?;

    Ok((
        StatusCode::CREATED,
        Json(CreateDeviceResponse {
            device: DeviceResponse {
                id: db.last_insert_rowid(),
                name,
                state: "offline".to_string(),
                updated_at: now,
            },
            token,
        }),
    ))
}

pub async fn device_list(
    State(state): State<AppState>,
    user: AuthUser,
) -> Result<Json<Vec<DeviceResponse>>, AppError> {
    let db = state.db.lock().unwrap();

    let mut stmt = db.prepare(
        "SELECT d.id, d.name,
              CASE WHEN COALESCE(s.updated_at, d.created_at) <= ?2 THEN 'offline'
              ELSE COALESCE(s.state, 'offline') END,
              COALESCE(s.updated_at, d.created_at)
              FROM devices d
              LEFT JOIN device_state s ON s.device_id = d.id
              WHERE d.user_id = ?1
              ORDER BY d.id",
    )?;

    let devices = stmt
        .query_map(
            params![user.id, unix_now() - DEVICE_ONLINE_TIMEOUT_SECS],
            |row| {
                let id = row.get(0)?;
                let name = row.get(1)?;
                let state = row.get(2)?;
                let updated_at = row.get(3)?;
                Ok(DeviceResponse {
                    id,
                    name,
                    state,
                    updated_at,
                })
            },
        )?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(Json(devices))
}

pub async fn device_update_state(
    State(state): State<AppState>,
    device: AuthDevice,
    Json(payload): Json<DeviceStateRequest>,
) -> Result<StatusCode, AppError> {
    let new_state = payload.state.trim();
    if !matches!(new_state, "offline" | "online" | "error" | "busy") {
        return Err(AppError::InvalidDeviceState);
    }
    let db = state.db.lock().unwrap();

    db.execute(
        "INSERT INTO device_state (device_id, state, updated_at)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(device_id) DO UPDATE
         SET state = excluded.state, updated_at = excluded.updated_at",
        params![device.id, new_state, unix_now()],
    )?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn device_get_state(
    State(state): State<AppState>,
    Path(device_id): Path<i64>,
    user: AuthUser,
) -> Result<Json<DeviceResponse>, AppError> {
    let db = state.db.lock().unwrap();

    match db.query_row(
        "SELECT d.name,
              CASE WHEN COALESCE(s.updated_at, d.created_at) <= ?3 THEN 'offline'
              ELSE COALESCE(s.state, 'offline') END,
              COALESCE(s.updated_at, d.created_at)
              FROM devices d
              LEFT JOIN device_state s ON s.device_id = d.id
              WHERE d.id = ?1 AND d.user_id = ?2",
        params![device_id, user.id, unix_now() - DEVICE_ONLINE_TIMEOUT_SECS],
        |row| {
            Ok(DeviceResponse {
                id: device_id,
                name: row.get(0)?,
                state: row.get(1)?,
                updated_at: row.get(2)?,
            })
        },
    ) {
        Ok(resp) => Ok(Json(resp)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Err(AppError::NotFound),
        Err(e) => Err(e.into()),
    }
}

pub async fn device_rename(
    State(state): State<AppState>,
    Path(device_id): Path<i64>,
    user: AuthUser,
    Json(payload): Json<CreateDeviceRequest>,
) -> Result<StatusCode, AppError> {
    let new_name = payload.name.trim().to_string();
    if new_name.is_empty() {
        return Err(AppError::InvalidDeviceName);
    }

    let db = state.db.lock().unwrap();

    let renamed = db.execute(
        "UPDATE devices 
        SET name = ?1
        WHERE id = ?2 AND user_id = ?3",
        params![new_name, device_id, user.id],
    )?;

    if renamed == 0 {
        return Err(AppError::NotFound);
    }

    Ok(StatusCode::NO_CONTENT)
}

pub async fn device_delete(
    State(state): State<AppState>,
    Path(device_id): Path<i64>,
    user: AuthUser,
) -> Result<StatusCode, AppError> {
    let db = state.db.lock().unwrap();

    let deleted = db.execute(
        "DELETE FROM devices WHERE id = ?1 AND user_id = ?2",
        params![device_id, user.id],
    )?;

    if deleted == 0 {
        return Err(AppError::NotFound);
    }

    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn auth_user() -> AuthUser {
        AuthUser {
            id: 1,
            email: "a@mail.ru".to_string(),
        }
    }

    // What the owner's device list shows for a device that last reported
    // `reported` at `updated_at`.
    async fn displayed_state(reported: &str, updated_at: i64) -> String {
        let state = AppState::new(db::init_memory().unwrap());
        {
            let conn = state.db.lock().unwrap();
            conn.execute(
                "INSERT INTO users (email, password_hash) VALUES ('a@mail.ru', 'x')",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO devices (user_id, name, token, created_at) VALUES (1, 'dev', 'tok', 1)",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO device_state (device_id, state, updated_at) VALUES (1, ?1, ?2)",
                params![reported, updated_at],
            )
            .unwrap();
        }

        let devices = device_list(State(state), auth_user()).await.unwrap();
        devices.0[0].state.clone()
    }

    #[tokio::test]
    async fn device_not_heard_from_shows_offline() {
        assert_eq!(displayed_state("online", unix_now() - 60).await, "offline");
        assert_eq!(displayed_state("busy", unix_now() - 60).await, "offline");
    }

    #[tokio::test]
    async fn fresh_report_keeps_its_state() {
        assert_eq!(displayed_state("online", unix_now()).await, "online");
        assert_eq!(displayed_state("busy", unix_now()).await, "busy");
    }
}
