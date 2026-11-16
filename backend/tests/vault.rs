//! The password vault: encryption at rest, reveal, the access log, a locked vault.

mod common;

use serde_json::{Value, json};
use sqlx::PgPool;

async fn get_json(url: String) -> Value {
    reqwest::get(url).await.unwrap().json().await.unwrap()
}

async fn send(method: reqwest::Method, url: String, body: Value) -> reqwest::Response {
    reqwest::Client::new()
        .request(method, url)
        .json(&body)
        .send()
        .await
        .unwrap()
}

#[sqlx::test]
async fn stores_encrypted_and_reveals_on_demand(db: PgPool) {
    let base = common::spawn_app(db.clone(), common::NOWHERE).await;

    let response = send(
        reqwest::Method::POST,
        format!("{base}/api/vault"),
        json!({
            "site": "https://www.SEEK.com.au/sign-in",
            "login_url": "https://www.seek.com.au/sign-in",
            "username": "me@example.com",
            "password": "correct horse battery staple"
        }),
    )
    .await;
    assert_eq!(response.status(), 201);
    let created: Value = response.json().await.unwrap();
    assert_eq!(created["site"], "seek.com.au");
    assert!(
        created.get("password").is_none(),
        "never return the password"
    );
    let id = created["id"].as_str().unwrap().to_string();

    // At rest it's ciphertext, not the password.
    let secret: Vec<u8> = sqlx::query_scalar("SELECT secret FROM credentials")
        .fetch_one(&db)
        .await
        .unwrap();
    assert!(!String::from_utf8_lossy(&secret).contains("battery"));

    // The list has no passwords; reveal has exactly one.
    let list = get_json(format!("{base}/api/vault")).await;
    assert!(!list.to_string().contains("battery"));
    let revealed: Value = send(
        reqwest::Method::POST,
        format!("{base}/api/vault/{id}/reveal"),
        json!({}),
    )
    .await
    .json()
    .await
    .unwrap();
    assert_eq!(revealed["password"], "correct horse battery staple");

    // Same site again is a conflict.
    let response = send(
        reqwest::Method::POST,
        format!("{base}/api/vault"),
        json!({ "site": "seek.com.au", "username": "x", "password": "y" }),
    )
    .await;
    assert_eq!(response.status(), 409);

    // Editing without a password keeps the old one.
    let response = send(
        reqwest::Method::PUT,
        format!("{base}/api/vault/{id}"),
        json!({ "site": "seek.com.au", "username": "new@example.com", "notes": "main account" }),
    )
    .await;
    assert_eq!(response.status(), 200);
    let revealed: Value = send(
        reqwest::Method::POST,
        format!("{base}/api/vault/{id}/reveal"),
        json!({}),
    )
    .await
    .json()
    .await
    .unwrap();
    assert_eq!(revealed["password"], "correct horse battery staple");

    // Every action is in the access log, newest first.
    let status = get_json(format!("{base}/api/vault/status")).await;
    assert_eq!(status["unlocked"], true);
    assert_eq!(status["entries"], 1);
    let actions: Vec<&str> = status["recent_access"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["action"].as_str().unwrap())
        .collect();
    assert_eq!(actions, ["reveal", "update", "reveal", "create"]);

    let response = reqwest::Client::new()
        .delete(format!("{base}/api/vault/{id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 204);
}

#[sqlx::test]
async fn validates_input(db: PgPool) {
    let base = common::spawn_app(db, common::NOWHERE).await;
    for body in [
        json!({ "site": "not a site", "username": "a", "password": "b" }),
        json!({ "site": "seek.com.au", "username": " ", "password": "b" }),
        json!({ "site": "seek.com.au", "username": "a", "password": "" }),
        json!({ "site": "seek.com.au", "username": "a", "password": "b", "login_url": "seek.com.au/login" }),
    ] {
        let response = send(reqwest::Method::POST, format!("{base}/api/vault"), body).await;
        assert_eq!(response.status(), 400);
    }

    let generated = get_json(format!("{base}/api/vault/generate?length=24")).await;
    assert_eq!(generated["password"].as_str().unwrap().len(), 24);
}

#[sqlx::test]
async fn a_locked_vault_says_so(db: PgPool) {
    let base = common::spawn_locked_vault_app(db).await;

    let status = get_json(format!("{base}/api/vault/status")).await;
    assert_eq!(status["unlocked"], false);

    let response = send(
        reqwest::Method::POST,
        format!("{base}/api/vault"),
        json!({ "site": "seek.com.au", "username": "a", "password": "b" }),
    )
    .await;
    assert_eq!(response.status(), 503);
    let body: Value = response.json().await.unwrap();
    assert!(body["error"].as_str().unwrap().contains("vault-init"));
}

#[sqlx::test]
async fn usual_login_is_stored_encrypted_and_never_returned(db: PgPool) {
    let base = common::spawn_app(db.clone(), common::NOWHERE).await;

    let empty = get_json(format!("{base}/api/vault/default")).await;
    assert_eq!(empty["set"], false);

    let response = send(
        reqwest::Method::PUT,
        format!("{base}/api/vault/default"),
        json!({ "username": "", "password": "my usual one" }),
    )
    .await;
    assert_eq!(response.status(), 200);
    let saved: Value = response.json().await.unwrap();
    assert_eq!(saved, json!({ "set": true, "username": "" }));

    let secret: Vec<u8> =
        sqlx::query_scalar("SELECT secret FROM integrations WHERE name = 'default_login'")
            .fetch_one(&db)
            .await
            .unwrap();
    assert!(!String::from_utf8_lossy(&secret).contains("usual"));
    assert!(
        !get_json(format!("{base}/api/vault/default"))
            .await
            .to_string()
            .contains("usual one")
    );

    let empty_password = send(
        reqwest::Method::PUT,
        format!("{base}/api/vault/default"),
        json!({ "username": "me@example.com", "password": "" }),
    )
    .await;
    assert_eq!(empty_password.status(), 400);

    let deleted = send(
        reqwest::Method::DELETE,
        format!("{base}/api/vault/default"),
        json!({}),
    )
    .await;
    assert_eq!(deleted.status(), 204);
    assert_eq!(
        get_json(format!("{base}/api/vault/default")).await["set"],
        false
    );
}
