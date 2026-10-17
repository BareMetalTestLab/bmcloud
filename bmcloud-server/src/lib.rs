pub mod auth;
pub mod db;
pub mod devices;
pub mod errors;
pub mod firmwares;
pub mod state;
pub mod users;

use axum::{
    Router,
    extract::DefaultBodyLimit,
    routing::{delete, get, post},
};

use devices::{
    device_create, device_delete, device_get_state, device_list, device_me, device_rename,
    device_update_state,
};

use firmwares::{
    device_download_firmware, device_next_job, device_report_job, list_jobs,
    upload_firmware_and_create_job,
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
        .route("/jobs", get(list_jobs))
        .route("/devices", get(device_list).post(device_create))
        .route("/device/{id}", delete(device_delete).patch(device_rename))
        .route("/device/{id}/state", get(device_get_state))
        .route("/device/me", get(device_me))
        .route("/device/state", post(device_update_state))
        .route("/device/{id}/flash", post(upload_firmware_and_create_job))
        .route("/device/firmware/{id}", get(device_download_firmware))
        .route("/device/job", get(device_next_job))
        .route("/device/job/{id}", post(device_report_job))
        .fallback_service(ServeDir::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/static"
        )))
        .layer(DefaultBodyLimit::max(8 * 1024 * 1024))
        .with_state(state)
}
