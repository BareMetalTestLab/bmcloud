use bmcloud_server::state::AppState;

use serde_json::json;

use axum::http::StatusCode;

async fn spawn_app() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let conn = bmcloud_server::db::init_memory().unwrap();
    let app = bmcloud_server::router(AppState::new(conn));

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    format!("http://{addr}")
}

async fn register_and_login(
    client: &reqwest::Client,
    base: &str,
    login: &str,
    email: &str,
) -> String {
    client
        .post(format!("{base}/register"))
        .json(&json!({
            "login": login,
            "email": email,
            "password": "SuperSecret1!"
        }))
        .send()
        .await
        .unwrap();

    let response = client
        .post(format!("{base}/login"))
        .json(&json!({
            "email": email,
            "password": "SuperSecret1!"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body: serde_json::Value = response.json().await.unwrap();
    body["token"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn register_returns_409_for_duplicate_login() {
    let base = spawn_app().await;

    // 1. Register a new user
    let response = reqwest::Client::new()
        .post(format!("{base}/register"))
        .json(&json!({
            "login": "anna",
            "email": "anna1@mail.ru",
            "password": "SuperSecret1!"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    // 2. Register the same user again
    let response = reqwest::Client::new()
        .post(format!("{base}/register"))
        .json(&json!({
            "login": "anna",
            "email": "anna2@mail.ru",
            "password": "SuperSecret1!"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["error"], "login is already taken");
}

#[tokio::test]
async fn login_returns_401_for_unknown_email() {
    let base = spawn_app().await;

    // 1. Register a new user
    let response = reqwest::Client::new()
        .post(format!("{base}/register"))
        .json(&json!({
            "login": "anna",
            "email": "anna@mail.ru",
            "password": "SuperSecret1!"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    // 2. Login the user with unknown email
    let response = reqwest::Client::new()
        .post(format!("{base}/login"))
        .json(&json!({
            "email": "paul@mail.ru",
            "password": "SuperSecret1!"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn login_returns_401_and_invalid_password() {
    let base = spawn_app().await;

    // 1. Register a new user
    let response = reqwest::Client::new()
        .post(format!("{base}/register"))
        .json(&json!({
            "login": "anna",
            "email": "anna@mail.ru",
            "password": "SuperSecret1!"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    // 2. Login the user with wrong password
    let response = reqwest::Client::new()
        .post(format!("{base}/login"))
        .json(&json!({
            "email": "anna@mail.ru",
            "password": "WrongPassword1!"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn register_returns_400_for_invalid_password() {
    let base = spawn_app().await;

    let response = reqwest::Client::new()
        .post(format!("{base}/register"))
        .json(&json!({
            "login": "anna",
            "email": "anna@mail.ru",
            "password": "short"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["error"], "password must be at least 8 characters");
}

#[tokio::test]
async fn duplicate_email_returns_409() {
    let base = spawn_app().await;
    let client = reqwest::Client::new();
    let payload = json!({
        "login": "anna",
        "email": "anna@mail.ru",
        "password": "SuperSecret1!"
    });

    client
        .post(format!("{base}/register"))
        .json(&payload)
        .send()
        .await
        .unwrap();

    let response = client
        .post(format!("{base}/register"))
        .json(&payload)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn root_returns_dashboard_html() {
    let base = spawn_app().await;

    let response = reqwest::Client::new()
        .get(format!("{base}/"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert!(response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .starts_with("text/html"));
    let body = response.text().await.unwrap();
    assert!(body.contains("My devices"));
    assert!(body.contains("app.js"));
}

#[tokio::test]
async fn unknown_path_returns_404() {
    let base = spawn_app().await;

    let response = reqwest::Client::new()
        .get(format!("{base}/unknown_path"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn login_returns_token_and_me_works() {
    let base = spawn_app().await;
    let client = reqwest::Client::new();

    // 1. register new valid user and login to get a token
    let token = register_and_login(&reqwest::Client::new(), &base, "anna", "anna@mail.ru").await;

    // 2. Call /me with the token
    let response = client
        .get(format!("{base}/me"))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["login"], "anna");
    assert_eq!(body["email"], "anna@mail.ru");

    // 3. Logout
    let response = reqwest::Client::new()
        .post(format!("{base}/logout"))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 204);

    // 4. Call /me again with the same token, should return 401
    let response = reqwest::Client::new()
        .get(format!("{base}/me"))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    // 5. Repeat logout with the same token, should return 401
    let response = reqwest::Client::new()
        .post(format!("{base}/logout"))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn me_without_token_returns_401() {
    let base = spawn_app().await;

    let response = reqwest::Client::new()
        .get(format!("{base}/me"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["error"], "missing or invalid token");
}

#[tokio::test]
async fn me_with_unknown_token_returns_401() {
    let base = spawn_app().await;

    let response = reqwest::Client::new()
        .get(format!("{base}/me"))
        .header("Authorization", "Bearer not-a-real-token")
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn create_device_returns_201_and_device_can_be_listed() {
    let base = spawn_app().await;

    // 1. register new valid user and login to get a token
    let token = register_and_login(&reqwest::Client::new(), &base, "anna", "anna@mail.ru").await;

    // 2. Create two devices
    let response = reqwest::Client::new()
        .post(format!("{base}/devices"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&json!({"name": "device_1_name"}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let response = reqwest::Client::new()
        .post(format!("{base}/devices"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&json!({"name": "device_2_name"}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    // 3. Get the list of devices
    let response = reqwest::Client::new()
        .get(format!("{base}/devices"))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = response.json().await.unwrap();
    let devices = body.as_array().unwrap();
    assert_eq!(devices.len(), 2);
    assert_eq!(devices.get(0).unwrap()["name"], "device_1_name");
    assert_eq!(devices.get(1).unwrap()["name"], "device_2_name");
    assert!(devices[0].get("token").is_none());

    // 4. Delete the first device
    let device_id = devices.get(0).unwrap()["id"].clone();
    let response = reqwest::Client::new()
        .delete(format!("{base}/device/{device_id}"))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 204);

    // 5. Get the list of devices
    let response = reqwest::Client::new()
        .get(format!("{base}/devices"))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = response.json().await.unwrap();
    let devices = body.as_array().unwrap();
    assert_eq!(devices.len(), 1);
    assert_eq!(devices.get(0).unwrap()["name"], "device_2_name");
}

#[tokio::test]
async fn create_device_without_name_returns_400() {
    let base = spawn_app().await;

    let token = register_and_login(&reqwest::Client::new(), &base, "anna", "anna@mail.ru").await;

    let response = reqwest::Client::new()
        .post(format!("{base}/devices"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&json!({"name": ""}))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn create_device_with_unknown_token_returns_401() {
    let base = spawn_app().await;

    let response = reqwest::Client::new()
        .post(format!("{base}/devices"))
        .header("Authorization", "Bearer not-a-real-token")
        .json(&json!({"name": "device_0"}))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn delete_unknown_device_returns_404() {
    let base = spawn_app().await;

    let token = register_and_login(&reqwest::Client::new(), &base, "anna", "anna@mail.ru").await;

    let response = reqwest::Client::new()
        .delete(format!("{base}/device/7777777"))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn devices_of_other_users_are_invisible() {
    let base = spawn_app().await;
    let client = reqwest::Client::new();

    let token_a = register_and_login(&client, &base, "anna", "anna@mail.ru").await;
    let token_b = register_and_login(&client, &base, "paul", "paul@mail.ru").await;

    // A create a device
    let response = client
        .post(format!("{base}/devices"))
        .header("Authorization", format!("Bearer {token_a}"))
        .json(&json!({ "name": "anna-esp32" }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    // B cannot see A's device
    let body: serde_json::Value = response.json().await.unwrap();
    let device_id = body["device"]["id"].as_i64().unwrap();

    let response = client
        .get(format!("{base}/devices"))
        .header("Authorization", format!("Bearer {token_b}"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body.as_array().unwrap().len(), 0);

    // B cannot delete A's device
    let response = client
        .delete(format!("{base}/device/{device_id}"))
        .header("Authorization", format!("Bearer {token_b}"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn device_can_authenticate_with_its_token() {
    let base = spawn_app().await;
    let client = reqwest::Client::new();

    let token = register_and_login(&client, &base, "anna", "anna@mail.ru").await;

    // 1. Create a device and remember its device token
    let response = client
        .post(format!("{base}/devices"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&json!({ "name": "esp32-dev" }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 201);
    let body: serde_json::Value = response.json().await.unwrap();
    let device_token = body["token"].as_str().unwrap().to_string();

    // 2. The device introduces itself to the server
    let response = client
        .get(format!("{base}/device/me"))
        .header("Authorization", format!("Bearer {device_token}"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["name"], "esp32-dev");
    assert!(body["id"].is_i64());
}

#[tokio::test]
async fn unknown_device_token_returns_401() {
    let base = spawn_app().await;

    let response = reqwest::Client::new()
        .get(format!("{base}/device/me"))
        .header("Authorization", "Bearer not-a-real-token")
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn user_token_is_not_a_device_token() {
    let base = spawn_app().await;
    let client = reqwest::Client::new();

    let token = register_and_login(&client, &base, "anna", "anna@mail.ru").await;

    let response = reqwest::Client::new()
        .get(format!("{base}/device/me"))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn deleted_device_token_stops_working() {
    let base = spawn_app().await;
    let client = reqwest::Client::new();

    let token = register_and_login(&client, &base, "anna", "anna@mail.ru").await;

    let response = client
        .post(format!("{base}/devices"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&json!({ "name": "esp32-dev" }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let body: serde_json::Value = response.json().await.unwrap();
    let device_id = body["device"]["id"].as_i64().unwrap();
    let device_token = body["token"].as_str().unwrap().to_string();

    let response = client
        .delete(format!("{base}/device/{device_id}"))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let response = client
        .get(format!("{base}/device/me"))
        .header("Authorization", format!("Bearer {device_token}"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn device_reports_state_and_owner_reads_it() {
    let base = spawn_app().await;
    let client = reqwest::Client::new();

    let token = register_and_login(&client, &base, "anna", "anna@mail.ru").await;

    // 1. Create a device
    let response = client
        .post(format!("{base}/devices"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&json!({ "name": "esp32-dev" }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let body: serde_json::Value = response.json().await.unwrap();
    let device_id = body["device"]["id"].as_i64().unwrap();
    let device_token = body["token"].as_str().unwrap().to_string();

    // 2. Device reports "online"
    let response = client
        .post(format!("{base}/device/state"))
        .header("Authorization", format!("Bearer {device_token}"))
        .json(&json!({ "state": "online" }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    // 3. Device reports again, overwriting the state (UPSERT)
    let response = client
        .post(format!("{base}/device/state"))
        .header("Authorization", format!("Bearer {device_token}"))
        .json(&json!({ "state": "busy" }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    // 4. Owner reads the latest state
    let response = client
        .get(format!("{base}/device/{device_id}/state"))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["state"], "busy");
    assert!(body["updated_at"].is_i64());
}

#[tokio::test]
async fn state_of_foreign_device_returns_404() {
    let base = spawn_app().await;
    let client = reqwest::Client::new();

    let token_a = register_and_login(&client, &base, "anna", "anna@mail.ru").await;
    let token_b = register_and_login(&client, &base, "paul", "paul@mail.ru").await;

    // A creates a device and reports a state
    let response = client
        .post(format!("{base}/devices"))
        .header("Authorization", format!("Bearer {token_a}"))
        .json(&json!({ "name": "anna-esp32" }))
        .send()
        .await
        .unwrap();
    let body: serde_json::Value = response.json().await.unwrap();
    let device_id = body["device"]["id"].as_i64().unwrap();
    let device_token = body["token"].as_str().unwrap().to_string();

    client
        .post(format!("{base}/device/state"))
        .header("Authorization", format!("Bearer {device_token}"))
        .json(&json!({ "state": "online" }))
        .send()
        .await
        .unwrap();

    // B cannot read it
    let response = client
        .get(format!("{base}/device/{device_id}/state"))
        .header("Authorization", format!("Bearer {token_b}"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn user_token_cannot_report_state() {
    let base = spawn_app().await;

    let token = register_and_login(&reqwest::Client::new(), &base, "anna", "anna@mail.ru").await;

    let response = reqwest::Client::new()
        .post(format!("{base}/device/state"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&json!({ "state": "fake" }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn state_without_report_returns_404() {
    let base = spawn_app().await;

    let client = reqwest::Client::new();

    let token = register_and_login(&client, &base, "anna", "anna@mail.ru").await;

    let response = client
        .get(format!("{base}/device/7777777/state"))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn empty_state_returns_400() {
    let base = spawn_app().await;

    let client = reqwest::Client::new();

    let token = register_and_login(&client, &base, "anna", "anna@mail.ru").await;

    // A creates a device
    let response = client
        .post(format!("{base}/devices"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&json!({ "name": "anna-esp32" }))
        .send()
        .await
        .unwrap();
    let body: serde_json::Value = response.json().await.unwrap();
    let device_token = body["token"].as_str().unwrap().to_string();

    let response = client
        .post(format!("{base}/device/state"))
        .header("Authorization", format!("Bearer {device_token}"))
        .json(&json!({ "state": "     " }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(
        body["error"],
        "invalid device state, expected offline|online|error|busy"
    );

    let response = client
        .post(format!("{base}/device/state"))
        .header("Authorization", format!("Bearer {device_token}"))
        .json(&json!({ "state": "sleeping" }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn rename_returns_204() {
    let base = spawn_app().await;

    let client = reqwest::Client::new();

    let token = register_and_login(&client, &base, "anna", "anna@mail.ru").await;

    // A creates a device
    let response = client
        .post(format!("{base}/devices"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&json!({ "name": "anna-esp32" }))
        .send()
        .await
        .unwrap();
    let body: serde_json::Value = response.json().await.unwrap();
    let device_id = body["device"]["id"].as_i64().unwrap();
    let device_token = body["token"].as_str().unwrap().to_string();

    let response = client
        .patch(format!("{base}/device/{device_id}"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&json!({ "name": "anna-esp32-c5" }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let response = client
        .get(format!("{base}/device/me"))
        .header("Authorization", format!("Bearer {device_token}"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["name"], "anna-esp32-c5");
    assert!(body["id"].is_i64());
}

#[tokio::test]
async fn rename_by_another_user_returns_401() {
    let base = spawn_app().await;

    let client = reqwest::Client::new();

    let token = register_and_login(&client, &base, "anna", "anna@mail.ru").await;

    // A creates a device
    let response = client
        .post(format!("{base}/devices"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&json!({ "name": "anna-esp32" }))
        .send()
        .await
        .unwrap();
    let body: serde_json::Value = response.json().await.unwrap();
    let device_id = body["device"]["id"].as_i64().unwrap();

    let response = client
        .patch(format!("{base}/device/{device_id}"))
        .header("Authorization", format!("Bearer another-token"))
        .json(&json!({ "name": "anna-esp32-c5" }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn rename_unknown_device_returns_400() {
    let base = spawn_app().await;

    let client = reqwest::Client::new();

    let token = register_and_login(&client, &base, "anna", "anna@mail.ru").await;

    let response = client
        .patch(format!("{base}/device/7777777"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&json!({ "name": "anna-esp3" }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn rename_with_empty_name_returns_400() {
    let base = spawn_app().await;

    let client = reqwest::Client::new();

    let token = register_and_login(&client, &base, "anna", "anna@mail.ru").await;

    // A creates a device
    let response = client
        .post(format!("{base}/devices"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&json!({ "name": "anna-esp32" }))
        .send()
        .await
        .unwrap();
    let body: serde_json::Value = response.json().await.unwrap();
    let device_id = body["device"]["id"].as_i64().unwrap();

    let response = client
        .patch(format!("{base}/device/{device_id}"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&json!({ "name": "     " }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
