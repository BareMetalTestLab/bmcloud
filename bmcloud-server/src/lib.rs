pub mod auth;
pub mod db;
pub mod devices;
pub mod errors;
pub mod state;
pub mod users;

use axum::{
    Router,
    routing::{delete, get, post},
};

use devices::{
    device_create, device_delete, device_get_state, device_list, device_me, device_rename,
    device_update_state,
};

use users::{login, logout, me, register};

use state::AppState;
use tower_http::services::ServeDir;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/register", post(register))
        .route("/login", post(login))
        .route("/logout", post(logout))
        .route("/me", get(me))
        .route("/devices", get(device_list).post(device_create))
        .route("/device/{id}", delete(device_delete).patch(device_rename))
        .route("/device/{id}/state", get(device_get_state))
        .route("/device/me", get(device_me))
        .route("/device/state", post(device_update_state))
        .fallback_service(ServeDir::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/static"
        )))
        .with_state(state)
}
