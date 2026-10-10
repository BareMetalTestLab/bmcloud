use crate::errors::AppError;
use crate::models::{
    AuthDevice, AuthUser, CreateDeviceRequest, CreateDeviceResponse, DeviceResponse,
    DeviceStateRequest, LoginRequest, LoginResponse, RegisterRequest, UserResponse,
};
use crate::state::AppState;

use std::time::{SystemTime, UNIX_EPOCH};

use uuid::Uuid;

use axum::{
    Json,
    extract::{FromRequestParts, Path, State},
    http::request::Parts,
    http::{HeaderMap, StatusCode},
};

use argon2::{
    Argon2,
    password_hash::{PasswordHasher, PasswordVerifier, phc::PasswordHash},
};

use rusqlite::params;

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let user = current_user(state, &parts.headers)?;
        Ok(user)
    }
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

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is after 1970")
        .as_secs() as i64
}

fn current_user(state: &AppState, headers: &HeaderMap) -> Result<AuthUser, AppError> {
    let db = state.db.lock().unwrap();

    let token = extract_bearer_token(headers)?;

    match db.query_row(
        "SELECT u.id, u.login, u.email
             FROM sessions s
             JOIN users u ON u.id = s.user_id
             WHERE s.token = ?1",
        params![token],
        |row| {
            Ok(AuthUser {
                id: row.get(0)?,
                login: row.get(1)?,
                email: row.get(2)?,
            })
        },
    ) {
        Ok(user) => return Ok(user),
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            return Err(AppError::Unauthorized);
        }
        Err(e) => return Err(e.into()),
    };
}

fn current_device(state: &AppState, headers: &HeaderMap) -> Result<AuthDevice, AppError> {
    let db = state.db.lock().unwrap();

    let token = extract_bearer_token(headers)?;

    match db.query_row(
        "SELECT d.id, d.name, COALESCE(s.state, 'offline'), COALESCE(s.updated_at, d.created_at), d.user_id
        FROM devices d
        LEFT JOIN device_state s ON s.device_id = d.id
        WHERE d.token = ?1",
        params![token],
        |row| {
            Ok(AuthDevice {
                info: DeviceResponse {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    state: row.get(2)?,
                    updated_at: row.get(3)?,
                },
                owner_id: row.get(4)?,
            })
        },
    ) {
        Ok(device) => return Ok(device),
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            return Err(AppError::Unauthorized);
        }
        Err(e) => return Err(e.into()),
    };
}

fn extract_bearer_token(headers: &HeaderMap) -> Result<String, AppError> {
    let header = headers.get("Authorization").ok_or(AppError::Unauthorized)?;

    let header = header.to_str().map_err(|_| AppError::Unauthorized)?;

    let token = header
        .strip_prefix("Bearer ")
        .ok_or(AppError::Unauthorized)?;

    Ok(token.to_string())
}

pub async fn register(
    State(state): State<AppState>,
    Json(payload): Json<RegisterRequest>,
) -> Result<(StatusCode, Json<UserResponse>), AppError> {
    payload.validate()?;

    let password_hash = Argon2::default()
        .hash_password(payload.password.as_bytes())
        .expect("argon2 no crash when password is valid")
        .to_string();

    let db = state.db.lock().unwrap();

    match db.execute(
        "INSERT INTO users (login, email, password_hash) VALUES (?1, ?2, ?3)",
        params![payload.login.clone(), payload.email.clone(), password_hash],
    ) {
        Ok(_) => {}
        Err(rusqlite::Error::SqliteFailure(_, Some(msg))) if msg.contains("users.email") => {
            return Err(AppError::EmailTaken);
        }
        Err(rusqlite::Error::SqliteFailure(_, Some(msg))) if msg.contains("users.login") => {
            return Err(AppError::LoginTaken);
        }
        Err(e) => return Err(e.into()),
    };

    let id = db.last_insert_rowid();

    Ok((
        StatusCode::CREATED,
        Json(UserResponse {
            id,
            login: payload.login,
            email: payload.email,
        }),
    ))
}

pub async fn login(
    State(state): State<AppState>,
    Json(payload): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, AppError> {
    let db = state.db.lock().unwrap();

    let (user, password_hash): (_, String) = match db.query_row(
        "SELECT id, login, email, password_hash FROM users WHERE email = ?1",
        params![payload.email],
        |row| {
            Ok((
                UserResponse {
                    id: row.get(0)?,
                    login: row.get(1)?,
                    email: row.get(2)?,
                },
                row.get(3)?,
            ))
        },
    ) {
        Ok((u, h)) => (u, h),
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            return Err(AppError::InvalidCredentials);
        }
        Err(e) => return Err(e.into()),
    };

    let parsed = PasswordHash::new(&password_hash).expect("hash stored in db is valid");

    Argon2::default()
        .verify_password(payload.password.as_bytes(), &parsed)
        .map_err(|_| AppError::InvalidCredentials)?;

    let token = Uuid::new_v4().to_string();

    db.execute(
        "INSERT INTO sessions (token, user_id, created_at) VALUES (?1, ?2, ?3)",
        params![token, user.id, unix_now()],
    )?;

    Ok(Json(LoginResponse { user, token }))
}

pub async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<StatusCode, AppError> {
    let token = extract_bearer_token(&headers)?;

    let db = state.db.lock().unwrap();

    let deleted = db.execute("DELETE FROM sessions WHERE token = ?1", params![token])?;
    if deleted == 0 {
        return Err(AppError::Unauthorized);
    }

    Ok(StatusCode::NO_CONTENT)
}

pub async fn me(user: AuthUser) -> Json<UserResponse> {
    Json(UserResponse {
        id: user.id,
        login: user.login,
        email: user.email,
    })
}

pub async fn create_device(
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

pub async fn delete_device(
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

pub async fn list_devices(
    State(state): State<AppState>,
    user: AuthUser,
) -> Result<Json<Vec<DeviceResponse>>, AppError> {
    let db = state.db.lock().unwrap();

    let mut stmt = db.prepare(
        "SELECT d.id, d.name, COALESCE(s.state, 'offline'), COALESCE(s.updated_at, d.created_at)
        FROM devices d
        LEFT JOIN device_state s ON s.device_id = d.id
        WHERE d.user_id = ?1
        ORDER BY d.id",
    )?;

    let devices = stmt
        .query_map(params![user.id], |row| {
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
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(Json(devices))
}

pub async fn device_me(device: AuthDevice) -> Json<DeviceResponse> {
    Json(DeviceResponse {
        id: device.info.id,
        name: device.info.name,
        state: device.info.state,
        updated_at: device.info.updated_at,
    })
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
        params![device.info.id, new_state, unix_now()],
    )?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn get_device_state(
    State(state): State<AppState>,
    Path(device_id): Path<i64>,
    user: AuthUser,
) -> Result<Json<DeviceResponse>, AppError> {
    let db = state.db.lock().unwrap();

    match db.query_row(
        "SELECT d.name, COALESCE(s.state, 'offline'), COALESCE(s.updated_at, d.created_at)
        FROM devices d
        LEFT JOIN device_state s ON s.device_id = d.id
        WHERE d.id = ?1 AND d.user_id = ?2",
        params![device_id, user.id],
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

pub async fn rename_device(
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn valid_request() -> RegisterRequest {
        RegisterRequest {
            login: "anna".to_string(),
            email: "anna@mail.ru".to_string(),
            password: "SuperSecret1!".to_string(),
        }
    }

    fn get_hash(db: &rusqlite::Connection, email: &str) -> String {
        db.query_row(
            "SELECT password_hash FROM users WHERE email = ?1",
            params![email],
            |row| row.get(0),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn same_password_gives_different_hashes() {
        let state = AppState::new(db::init_memory().unwrap());

        let req1 = RegisterRequest {
            login: "anna1".to_string(),
            email: "anna1@mail.ru".to_string(),
            ..valid_request()
        };
        let req2 = RegisterRequest {
            login: "anna2".to_string(),
            email: "anna2@mail.ru".to_string(),
            ..valid_request()
        };

        let _ = register(State(state.clone()), Json(req1.clone()))
            .await
            .unwrap();
        let _ = register(State(state.clone()), Json(req2.clone()))
            .await
            .unwrap();

        let db = state.db.lock().unwrap();

        let hash1: String = get_hash(&db, &req1.email);
        let hash2: String = get_hash(&db, &req2.email);

        assert_ne!(hash1, hash2);
    }

    #[tokio::test]
    async fn register_returns_201_for_new_user() {
        let state = AppState::new(db::init_memory().unwrap());

        let response = register(State(state.clone()), Json(valid_request()))
            .await
            .unwrap();

        let db = state.db.lock().unwrap();
        let hash = get_hash(&db, &valid_request().email);

        assert_ne!(hash, "SuperSecret1!");
        assert!(hash.starts_with("$argon2id"));

        assert_eq!(response.1.0.id, 1);
        assert_eq!(response.1.0.login, "anna");
    }

    #[tokio::test]
    async fn register_returns_409_for_duplicate_email() {
        let state = AppState::new(db::init_memory().unwrap());

        let first = register(State(state.clone()), Json(valid_request())).await;
        assert!(first.is_ok());

        let second = register(State(state), Json(valid_request())).await;
        assert_eq!(second.unwrap_err(), AppError::EmailTaken);
    }
}
