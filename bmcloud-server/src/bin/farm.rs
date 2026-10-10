use rand::{rng, RngExt};
use serde_json::json;
use tokio::time::{interval, Duration};

const BASE: &str = "http://localhost:3000";
const EMAIL: &str = "farmer@bmcloud.local";
const PASSWORD: &str = "Farmer1!x";
const COUNT: usize = 10;

#[tokio::main]
async fn main() {
    let client = reqwest::Client::new();

    // 1. User with farm: 409 = "already exist" — it's not error
    let resp = client
        .post(format!("{BASE}/register"))
        .json(&json!({ "login": "farmer", "email": EMAIL, "password": PASSWORD }))
        .send()
        .await
        .unwrap();
    match resp.status() {
        reqwest::StatusCode::CREATED | reqwest::StatusCode::CONFLICT => {}
        status => panic!("register failed: {status}"),
    }

    let resp = client
        .post(format!("{BASE}/login"))
        .json(&json!({ "email": EMAIL, "password": PASSWORD }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    let session = body["token"].as_str().unwrap().to_string();

    // 2. Create the devices and take its tokens
    let mut tokens = Vec::new();
    for i in 1..=COUNT {
        let resp = client
            .post(format!("{BASE}/devices"))
            .header("Authorization", format!("Bearer {session}"))
            .json(&json!({ "name": format!("esp32-sim-{i:02}") }))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 201, "create device {i} failed");
        let body: serde_json::Value = resp.json().await.unwrap();
        tokens.push(body["token"].as_str().unwrap().to_string());
    }
    println!("создано устройств: {}", tokens.len());

    // 3. Each device is separate task
    for (i, token) in tokens.into_iter().enumerate() {
        let client = client.clone();
        tokio::spawn(async move {
            let states = ["online", "busy", "online", "error", "offline"];
            let period = 2 + (i % 5) as u64;
            let mut ticker = interval(Duration::from_secs(period));
            loop {
                ticker.tick().await;
                let state = states[rng().random_range(0..states.len())];
                let resp = client
                    .post(format!("{BASE}/device/state"))
                    .header("Authorization", format!("Bearer {token}"))
                    .json(&json!({ "state": state }))
                    .send()
                    .await;
                match resp {
                    Ok(r) if r.status() == 204 => {}
                    other => {
                        eprintln!(
                            "esp32-sim-{:02}: failed post ({:?}), device is stopping",
                            i + 1, other
                        );
                        return;
                    }
                }
            }
        });
    }

    println!("farm is running: {COUNT} devices respond every 2–6 s");
    println!("open http://localhost:3000 and login {EMAIL} / {PASSWORD}");
    println!("Ctrl+C — to stop");
    tokio::signal::ctrl_c().await.unwrap();
    println!("farm has been stopped");
}
