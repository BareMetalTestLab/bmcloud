use crate::auth::{AuthDevice, AuthUser, unix_now};
use crate::errors::AppError;
use crate::state::AppState;

use axum::{
    Json,
    body::Bytes,
    extract::{Path, Query, State},
    http::{HeaderName, StatusCode, header},
};

use rusqlite::params;

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct FlashQuery {
    name: String,
}

#[derive(Debug, Serialize)]
pub struct FlashResponse {
    job_id: i64,
    firmware_id: i64,
    name: String,
    size: i64,
}

#[derive(Debug, Serialize)]
pub struct DeviceJobResponse {
    job_id: i64,
    firmware_id: i64,
}

#[derive(Debug, Deserialize)]
pub struct JobReportRequest {
    status: String,
    error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct JobResponse {
    job_id: i64,
    device_id: i64,
    fw_name: String,
    status: String,
    error: Option<String>,
}

pub async fn upload_firmware_and_create_job(
    State(state): State<AppState>,
    Path(device_id): Path<i64>,
    user: AuthUser,
    Query(query): Query<FlashQuery>,
    body: Bytes,
) -> Result<(StatusCode, Json<FlashResponse>), AppError> {
    let name = query.name.trim().to_string();
    if name.is_empty() || body.is_empty() {
        return Err(AppError::InvalidFirmwareName);
    }

    let db = state.db.lock().unwrap();

    let owner: i64 = db
        .query_row(
            "SELECT user_id FROM devices WHERE id = ?1",
            params![device_id],
            |row| row.get(0),
        )
        .map_err(|_| AppError::NotFound)?;
    if owner != user.id {
        return Err(AppError::NotFound);
    }

    let now = unix_now();

    let tx = db.unchecked_transaction()?;
    tx.execute(
        "INSERT INTO firmwares (user_id, name, size, data, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![user.id, name, body.len() as i64, &body[..], now],
    )?;
    let firmware_id = db.last_insert_rowid();
    tx.execute(
        "INSERT INTO flash_jobs (device_id, firmware_id, status, created_at, updated_at)
         VALUES (?1, ?2, 'pending', ?3, ?3)",
        params![device_id, firmware_id, now],
    )?;
    let job_id = db.last_insert_rowid();
    tx.commit()?;

    Ok((
        StatusCode::CREATED,
        Json(FlashResponse {
            job_id,
            firmware_id,
            name,
            size: body.len() as i64,
        }),
    ))
}

pub async fn device_next_job(
    State(state): State<AppState>,
    device: AuthDevice,
) -> Result<Json<DeviceJobResponse>, AppError> {
    let db = state.db.lock().unwrap();

    let job: Option<(i64, i64)> = db
        .query_row(
            "SELECT id, firmware_id FROM flash_jobs
             WHERE device_id = ?1 AND status = 'pending'
             ORDER BY id LIMIT 1",
            params![device.id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map(Some)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other),
        })?;

    let Some((job_id, firmware_id)) = job else {
        return Err(AppError::NotFound);
    };

    db.execute(
        "UPDATE flash_jobs SET status = 'flashing', updated_at = ?2 WHERE id = ?1",
        params![job_id, unix_now()],
    )?;

    Ok(Json(DeviceJobResponse {
        job_id,
        firmware_id,
    }))
}

pub async fn device_download_firmware(
    State(state): State<AppState>,
    Path(firmware_id): Path<i64>,
    device: AuthDevice,
) -> Result<([(HeaderName, &'static str); 1], Vec<u8>), AppError> {
    let db = state.db.lock().unwrap();

    let data: Vec<u8> = db
        .query_row(
            "SELECT f.data FROM firmwares f
             JOIN devices d ON d.user_id = f.user_id
             WHERE f.id = ?1 AND d.id = ?2",
            params![firmware_id, device.id],
            |row| row.get(0),
        )
        .map_err(|_| AppError::NotFound)?;

    Ok(([(header::CONTENT_TYPE, "application/octet-stream")], data))
}

pub async fn device_report_job(
    State(state): State<AppState>,
    Path(job_id): Path<i64>,
    device: AuthDevice,
    Json(payload): Json<JobReportRequest>,
) -> Result<StatusCode, AppError> {
    if !matches!(payload.status.as_str(), "done" | "failed") {
        return Err(AppError::InvalidJobStatus);
    }

    let db = state.db.lock().unwrap();
    let updated = db.execute(
        "UPDATE flash_jobs SET status = ?1, error = ?2, updated_at = ?3
         WHERE id = ?4 AND device_id = ?5 AND status = 'flashing'",
        params![payload.status, payload.error, unix_now(), job_id, device.id],
    )?;

    if updated == 0 {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

pub async fn list_jobs(
    State(state): State<AppState>,
    user: AuthUser,
) -> Result<Json<Vec<JobResponse>>, AppError> {
    let db = state.db.lock().unwrap();

    let mut stmt = db.prepare(
        "SELECT j.id, j.device_id, f.name, j.status, j.error
              FROM flash_jobs j
              JOIN devices d ON d.id = j.device_id
              JOIN firmwares f ON f.id = j.firmware_id
              WHERE d.user_id = ?1
              ORDER BY j.id DESC",
    )?;

    let jobs = stmt
        .query_map(params![user.id], |row| {
            let job_id = row.get(0)?;
            let device_id = row.get(1)?;
            let fw_name = row.get(2)?;
            let status = row.get(3)?;
            let error = row.get(4)?;
            Ok(JobResponse {
                job_id,
                device_id,
                fw_name,
                status,
                error,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(Json(jobs))
}
