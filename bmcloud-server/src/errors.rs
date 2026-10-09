use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};

use serde::Serialize;

#[derive(Debug, PartialEq)]
pub enum AppError {
    NotFound,
    InvalidEmail,
    EmailTaken,
    LoginTaken,
    InvalidLogin,
    WeakPassword,
    PasswordNoDigit,
    PasswordNoLetter,
    PasswordNoUpperCase,
    PasswordNoLowerCase,
    PasswordNoPunctuation,
    PasswordHasNonAscii,
    InvalidCredentials,
    Unauthorized,
    Database,
    InvalidDeviceName,
    InvalidDeviceState,
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        eprintln!("database error: {e}");
        AppError::Database
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            AppError::NotFound => (StatusCode::NOT_FOUND, "not found"),
            AppError::InvalidEmail => (StatusCode::BAD_REQUEST, "invalid email"),
            AppError::EmailTaken => (StatusCode::CONFLICT, "email is already taken"),
            AppError::LoginTaken => (StatusCode::CONFLICT, "login is already taken"),
            AppError::InvalidLogin => (StatusCode::BAD_REQUEST, "login must not be empty"),
            AppError::WeakPassword => (
                StatusCode::BAD_REQUEST,
                "password must be at least 8 characters",
            ),
            AppError::PasswordNoDigit => (StatusCode::BAD_REQUEST, "password must contain a digit"),
            AppError::PasswordNoLetter => {
                (StatusCode::BAD_REQUEST, "password must contain a letter")
            }
            AppError::PasswordNoUpperCase => (
                StatusCode::BAD_REQUEST,
                "password must contain an uppercase letter",
            ),
            AppError::PasswordNoLowerCase => (
                StatusCode::BAD_REQUEST,
                "password must contain a lowercase letter",
            ),
            AppError::PasswordNoPunctuation => (
                StatusCode::BAD_REQUEST,
                "password must contain a punctuation",
            ),
            AppError::PasswordHasNonAscii => (
                StatusCode::BAD_REQUEST,
                "password must contain only ASCII characters",
            ),
            AppError::InvalidCredentials => (StatusCode::UNAUTHORIZED, "invalid email or password"),
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, "missing or invalid token"),
            AppError::Database => (StatusCode::INTERNAL_SERVER_ERROR, "internal server error"),
            AppError::InvalidDeviceName => (StatusCode::BAD_REQUEST, "invalid device name"),
            AppError::InvalidDeviceState => (
                StatusCode::BAD_REQUEST,
                "invalid device state, expected offline|online|error|busy",
            ),
        };

        (
            status,
            Json(ErrorResponse {
                error: message.to_string(),
            }),
        )
            .into_response()
    }
}
