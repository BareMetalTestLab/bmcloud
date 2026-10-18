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

/// How long a claimed job may stay in `flashing` without a report before it
/// is considered dead (the device crashed or rebooted mid-flash) and failed.
const FLASH_REPORT_TIMEOUT_SECS: i64 = 10 * 60;

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
    let now = unix_now();

    // A claimed job whose device died mid-flash never reports back, so it is
    // reaped here instead of flashing forever: straight to failed, no retry -
    // the user re-flashes manually if needed.
    db.execute(
        "UPDATE flash_jobs SET status = 'failed', updated_at = ?1,
              error = 'no report from device'
              WHERE device_id = ?2 AND status = 'flashing'
              AND updated_at <= ?1 - ?3",
        params![now, device.id, FLASH_REPORT_TIMEOUT_SECS],
    )?;

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
        params![job_id, now],
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

    // The same reap as in device_next_job, from the user's side: a job whose
    // device never came back fails here too, so the dashboard does not show
    // an eternal "flashing" row.
    db.execute(
        "UPDATE flash_jobs SET status = 'failed', updated_at = ?1,
              error = 'no report from device'
              WHERE status = 'flashing' AND updated_at <= ?1 - ?2
              AND device_id IN (SELECT id FROM devices WHERE user_id = ?3)",
        params![unix_now(), FLASH_REPORT_TIMEOUT_SECS, user.id],
    )?;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    // A job claimed by a device that went silent: status flashing, last
    // updated long before the report timeout.
    fn silent_device_job(conn: &rusqlite::Connection) {
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
            "INSERT INTO firmwares (user_id, name, size, data, created_at)
             VALUES (1, 'fw.bin', 1, X'01', 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO flash_jobs (device_id, firmware_id, status, error, created_at, updated_at)
             VALUES (1, 1, 'flashing', NULL, 1, ?1)",
            params![unix_now() - FLASH_REPORT_TIMEOUT_SECS - 1],
        )
        .unwrap();
    }

    fn test_device() -> AuthDevice {
        AuthDevice {
            id: 1,
            name: "dev".to_string(),
        }
    }

    #[tokio::test]
    async fn stale_flashing_job_is_failed_without_retry() {
        let state = AppState::new(db::init_memory().unwrap());
        {
            let conn = state.db.lock().unwrap();
            silent_device_job(&conn);
        }

        // no pending work for the device - the stale job did not come back
        let resp = device_next_job(State(state.clone()), test_device()).await;
        assert_eq!(resp.unwrap_err(), AppError::NotFound);

        let conn = state.db.lock().unwrap();
        let (status, error): (String, Option<String>) = conn
            .query_row(
                "SELECT status, error FROM flash_jobs WHERE id = 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(status, "failed");
        assert_eq!(error.as_deref(), Some("no report from device"));
    }

    #[tokio::test]
    async fn stale_flashing_job_fails_in_the_user_job_list() {
        let state = AppState::new(db::init_memory().unwrap());
        {
            let conn = state.db.lock().unwrap();
            silent_device_job(&conn);
        }

        let jobs = list_jobs(
            State(state.clone()),
            AuthUser {
                id: 1,
                email: "a@mail.ru".to_string(),
            },
        )
        .await
        .unwrap();

        assert_eq!(jobs.0.len(), 1);
        assert_eq!(jobs.0[0].status, "failed");
        assert_eq!(jobs.0[0].error.as_deref(), Some("no report from device"));
    }

    #[tokio::test]
    async fn fresh_flashing_job_is_not_reaped() {
        let state = AppState::new(db::init_memory().unwrap());
        {
            let conn = state.db.lock().unwrap();
            silent_device_job(&conn);
            // the same job, but claimed just now - the timeout has not passed
            conn.execute("UPDATE flash_jobs SET updated_at = ?1", params![unix_now()])
                .unwrap();
        }

        let resp = device_next_job(State(state.clone()), test_device()).await;
        // no pending jobs and the live flashing one is left alone
        assert_eq!(resp.unwrap_err(), AppError::NotFound);
        let conn = state.db.lock().unwrap();
        let status: String = conn
            .query_row("SELECT status FROM flash_jobs WHERE id = 1", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(status, "flashing");
    }
}
