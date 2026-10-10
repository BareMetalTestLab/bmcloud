pub mod db;
pub mod errors;
pub mod handlers;
pub mod models;
pub mod state;

use axum::{
    Router,
    routing::{delete, get, post},
};

use handlers::{
    create_device, delete_device, device_me, device_update_state, get_device_state, list_devices,
    login, logout, me, register, rename_device,
};
use state::AppState;
use tower_http::services::ServeDir;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/register", post(register))
        .route("/login", post(login))
        .route("/logout", post(logout))
        .route("/me", get(me))
        .route("/devices", get(list_devices).post(create_device))
        .route("/device/{id}", delete(delete_device).patch(rename_device))
        .route("/device/{id}/state", get(get_device_state))
        .route("/device/me", get(device_me))
        .route("/device/state", post(device_update_state))
        .fallback_service(ServeDir::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/static"
        )))
        .with_state(state)
}
