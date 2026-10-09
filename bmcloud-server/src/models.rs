use crate::errors::AppError;
use serde::{Deserialize, Serialize};

// User
#[derive(Deserialize, Clone)]
pub struct RegisterRequest {
    pub login: String,
    pub email: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct UserResponse {
    pub id: i64,
    pub login: String,
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

pub struct AuthUser {
    pub id: i64,
    pub login: String,
    pub email: String,
}

// Devices
#[derive(Debug, Deserialize)]
pub struct CreateDeviceRequest {
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct CreateDeviceResponse {
    pub device: DeviceResponse,
    pub token: String,
}

#[derive(Debug, Serialize)]
pub struct DeviceResponse {
    pub id: i64,
    pub name: String,
    pub state: String,
    pub updated_at: i64,
}
pub struct AuthDevice {
    pub info: DeviceResponse,
    pub owner_id: i64,
}

#[derive(Debug, Deserialize)]
pub struct DeviceStateRequest {
    pub state: String,
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
        if self.login.is_empty() {
            return Err(AppError::InvalidLogin);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_request() -> RegisterRequest {
        RegisterRequest {
            login: "anna".to_string(),
            email: "anna@mail.ru".to_string(),
            password: "SuperSecret1!".to_string(),
        }
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
}
