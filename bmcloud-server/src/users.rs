use crate::auth::{AuthUser, extract_bearer_token, unix_now};
use crate::errors::AppError;
use crate::state::AppState;

use uuid::Uuid;

use axum::{
    Json,
    extract::{FromRequestParts, State},
    http::request::Parts,
    http::{HeaderMap, HeaderValue, StatusCode, header::SET_COOKIE},
};

use argon2::{
    Argon2,
    password_hash::{PasswordHasher, PasswordVerifier, phc::PasswordHash},
};

use rusqlite::params;

use serde::{Deserialize, Serialize};

#[derive(Deserialize, Clone)]
pub struct RegisterRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct UserResponse {
    pub id: i64,
    pub email: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub user: UserResponse,
    pub token: String,
}

impl RegisterRequest {
    pub fn validate(&self) -> Result<(), AppError> {
        if !(self.email.contains('@') && self.email.contains('.')) {
            return Err(AppError::InvalidEmail);
        }
        if self.password.chars().any(|c| !c.is_ascii()) {
            return Err(AppError::PasswordHasNonAscii);
        }
        if self.password.chars().count() < 8 {
            return Err(AppError::WeakPassword);
        }
        if !self.password.chars().any(|c| c.is_ascii_digit()) {
            return Err(AppError::PasswordNoDigit);
        }
        if !self.password.chars().any(|c| c.is_ascii_alphabetic()) {
            return Err(AppError::PasswordNoLetter);
        }
        if !self.password.chars().any(|c| c.is_ascii_uppercase()) {
            return Err(AppError::PasswordNoUpperCase);
        }
        if !self.password.chars().any(|c| c.is_ascii_lowercase()) {
            return Err(AppError::PasswordNoLowerCase);
        }
        if !self.password.chars().any(|c| c.is_ascii_punctuation()) {
            return Err(AppError::PasswordNoPunctuation);
        }
        Ok(())
    }
}

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

const SESSION_COOKIE: &str = "bmcloud_session";
const SESSION_MAX_AGE: i64 = 7 * 24 * 60 * 60;

fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    let cookies = headers.get("Cookie")?.to_str().ok()?;
    for pair in cookies.split(';') {
        let pair = pair.trim();
        if let Some(value) = pair.strip_prefix(&format!("{name}=")) {
            return Some(value.to_string());
        }
    }
    None
}

fn extract_session_token(headers: &HeaderMap) -> Result<String, AppError> {
    if let Ok(token) = extract_bearer_token(headers) {
        return Ok(token);
    }
    cookie_value(headers, SESSION_COOKIE).ok_or(AppError::Unauthorized)
}

fn current_user(state: &AppState, headers: &HeaderMap) -> Result<AuthUser, AppError> {
    let db = state.db.lock().unwrap();

    let token = extract_session_token(headers)?;

    match db.query_row(
        "SELECT u.id, u.email
             FROM sessions s
             JOIN users u ON u.id = s.user_id
             WHERE s.token = ?1 AND s.created_at > ?2",
        params![token, unix_now() - SESSION_MAX_AGE],
        |row| {
            Ok(AuthUser {
                id: row.get(0)?,
                email: row.get(1)?,
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
        "INSERT INTO users (email, password_hash) VALUES (?1, ?2)",
        params![payload.email.clone(), password_hash],
    ) {
        Ok(_) => {}
        Err(rusqlite::Error::SqliteFailure(_, Some(msg))) if msg.contains("users.email") => {
            return Err(AppError::EmailTaken);
        }
        Err(e) => return Err(e.into()),
    };

    let id = db.last_insert_rowid();

    Ok((
        StatusCode::CREATED,
        Json(UserResponse {
            id,
            email: payload.email,
        }),
    ))
}

pub async fn login(
    State(state): State<AppState>,
    Json(payload): Json<LoginRequest>,
) -> Result<(HeaderMap, Json<LoginResponse>), AppError> {
    let db = state.db.lock().unwrap();

    let (user, password_hash): (_, String) = match db.query_row(
        "SELECT id, email, password_hash FROM users WHERE email = ?1",
        params![payload.email],
        |row| {
            Ok((
                UserResponse {
                    id: row.get(0)?,
                    email: row.get(1)?,
                },
                row.get(2)?,
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
    let now = unix_now();

    db.execute(
        "INSERT INTO sessions (token, user_id, created_at) VALUES (?1, ?2, ?3)",
        params![token, user.id, now],
    )?;

    let cookie = format!(
        "{SESSION_COOKIE}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={SESSION_MAX_AGE}"
    );
    let mut headers = HeaderMap::new();
    headers.insert(SET_COOKIE, HeaderValue::from_str(&cookie).unwrap());

    db.execute(
        "DELETE FROM sessions WHERE created_at <= ?1",
        params![now - SESSION_MAX_AGE],
    )?;

    Ok((headers, Json(LoginResponse { user, token })))
}

pub async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<(HeaderMap, StatusCode), AppError> {
    let token = extract_session_token(&headers)?;

    let db = state.db.lock().unwrap();

    let deleted = db.execute("DELETE FROM sessions WHERE token = ?1", params![token])?;
    if deleted == 0 {
        return Err(AppError::Unauthorized);
    }

    let mut response_headers = HeaderMap::new();
    response_headers.insert(
        SET_COOKIE,
        HeaderValue::from_str(&format!(
            "{SESSION_COOKIE}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0"
        ))
        .unwrap(),
    );

    Ok((response_headers, StatusCode::NO_CONTENT))
}

pub async fn me(user: AuthUser) -> Json<UserResponse> {
    Json(UserResponse {
        id: user.id,
        email: user.email,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    use axum::http::header::AUTHORIZATION;

    fn valid_request() -> RegisterRequest {
        RegisterRequest {
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

    #[test]
    fn email_without_at_is_rejected() {
        let req = RegisterRequest {
            email: "anna-mail.ru".to_string(),
            ..valid_request()
        };
        assert_eq!(req.validate(), Err(AppError::InvalidEmail));
    }

    #[test]
    fn short_password_is_rejected() {
        let req = RegisterRequest {
            password: "1234567".to_string(),
            ..valid_request()
        };
        assert_eq!(req.validate(), Err(AppError::WeakPassword));
    }

    #[test]
    fn short_without_letter_is_rejected() {
        let req = RegisterRequest {
            password: "12345678!".to_string(),
            ..valid_request()
        };
        assert_eq!(req.validate(), Err(AppError::PasswordNoLetter));
    }

    #[test]
    fn password_without_digit_is_rejected() {
        let req = RegisterRequest {
            password: "SuperSecret!".to_string(),
            ..valid_request()
        };
        assert_eq!(req.validate(), Err(AppError::PasswordNoDigit));
    }

    #[test]
    fn password_without_uppercase_is_rejected() {
        let req = RegisterRequest {
            password: "supersecret1!".to_string(),
            ..valid_request()
        };
        assert_eq!(req.validate(), Err(AppError::PasswordNoUpperCase));
    }

    #[test]
    fn password_without_lowercase_is_rejected() {
        let req = RegisterRequest {
            password: "SUPERSECRET1!".to_string(),
            ..valid_request()
        };
        assert_eq!(req.validate(), Err(AppError::PasswordNoLowerCase));
    }

    #[test]
    fn password_without_punctuation_is_rejected() {
        let req = RegisterRequest {
            password: "SuperSecret1".to_string(),
            ..valid_request()
        };
        assert_eq!(req.validate(), Err(AppError::PasswordNoPunctuation));
    }

    #[test]
    fn cyrillic_password_is_rejected() {
        let req = RegisterRequest {
            password: "паро".to_string(),
            ..valid_request()
        };
        assert_eq!(req.validate(), Err(AppError::PasswordHasNonAscii));
    }

    #[test]
    fn mixed_password_with_cyrillic_is_rejected() {
        let req = RegisterRequest {
            password: "SuperSecret1!Пароль".to_string(),
            ..valid_request()
        };
        assert_eq!(req.validate(), Err(AppError::PasswordHasNonAscii));
    }

    #[tokio::test]
    async fn same_password_gives_different_hashes() {
        let state = AppState::new(db::init_memory().unwrap());

        let req1 = RegisterRequest {
            email: "anna1@mail.ru".to_string(),
            ..valid_request()
        };
        let req2 = RegisterRequest {
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
        assert_eq!(response.1.0.email, "anna@mail.ru");
    }

    #[tokio::test]
    async fn register_returns_409_for_duplicate_email() {
        let state = AppState::new(db::init_memory().unwrap());

        let first = register(State(state.clone()), Json(valid_request())).await;
        assert!(first.is_ok());

        let second = register(State(state), Json(valid_request())).await;
        assert_eq!(second.unwrap_err(), AppError::EmailTaken);
    }

    #[tokio::test]
    async fn old_session_returns_401() {
        let state = AppState::new(db::init_memory().unwrap());

        let reg = register(State(state.clone()), Json(valid_request())).await;
        assert!(reg.is_ok());

        let login_reg = login(
            State(state.clone()),
            Json(LoginRequest {
                email: valid_request().email,
                password: valid_request().password,
            }),
        )
        .await
        .unwrap();

        let token = login_reg.1.0.token;
        {
            let db = state.db.lock().unwrap();

            db.execute(
                "UPDATE sessions SET created_at = ?1 WHERE token = ?2",
                params![1, token],
            )
            .unwrap();
        }
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {token}")).unwrap(),
        );

        assert_eq!(
            current_user(&state, &headers).unwrap_err(),
            AppError::Unauthorized
        );
    }
}
