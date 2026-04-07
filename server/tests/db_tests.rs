//! Database and authentication tests

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::Value;
use tower::ServiceExt;

mod common;
use common::create_test_app_with_db;

#[tokio::test]
async fn test_login_with_database() {
    let app = create_test_app_with_db().await;

    // Login with default admin user
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"username": "admin", "password": "admin"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert!(json.get("token").is_some());
    assert!(json.get("expires_at").is_some());
    assert_eq!(json["user"]["username"], "admin");
    assert_eq!(json["user"]["role"], "admin");
}

#[tokio::test]
async fn test_login_wrong_password() {
    let app = create_test_app_with_db().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"username": "admin", "password": "wrong"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_login_unknown_user() {
    let app = create_test_app_with_db().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"username": "unknown", "password": "password"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_auth_flow_with_valid_token() {
    use hft_server::auth::{generate_token, AuthConfig};
    use hft_server::db::User;

    let app = create_test_app_with_db().await;

    // Generate a valid token manually for testing
    let user = User {
        id: 1,
        username: "admin".to_string(),
        password_hash: "".to_string(),
        role: "admin".to_string(),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    let auth_config = AuthConfig {
        jwt_secret: "test-secret-key".to_string(),
        token_expiry_hours: 24,
    };

    let token = generate_token(&user, &auth_config).unwrap();

    // Use the token to access /api/auth/me
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/auth/me")
                .header("authorization", format!("Bearer {}", token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["username"], "admin");
    assert_eq!(json["role"], "admin");
}

#[tokio::test]
async fn test_password_hashing() {
    use hft_server::auth::{hash_password, verify_password};

    let password = "test_password_123";
    let hash = hash_password(password).unwrap();

    // Hash should be different from password
    assert_ne!(hash, password);

    // Verify should return true for correct password
    assert!(verify_password(password, &hash).unwrap());

    // Verify should return false for wrong password
    assert!(!verify_password("wrong_password", &hash).unwrap());
}

#[tokio::test]
async fn test_jwt_token_generation() {
    use hft_server::auth::{generate_token, verify_token, AuthConfig};
    use hft_server::db::User;

    let user = User {
        id: 42,
        username: "testuser".to_string(),
        password_hash: "".to_string(),
        role: "operator".to_string(),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    let config = AuthConfig {
        jwt_secret: "test-secret".to_string(),
        token_expiry_hours: 24,
    };

    let token = generate_token(&user, &config).unwrap();
    let claims = verify_token(&token, &config).unwrap();

    assert_eq!(claims.sub, 42);
    assert_eq!(claims.username, "testuser");
    assert_eq!(claims.role, "operator");
}

#[tokio::test]
async fn test_jwt_token_wrong_secret() {
    use hft_server::auth::{generate_token, verify_token, AuthConfig};
    use hft_server::db::User;

    let user = User {
        id: 1,
        username: "admin".to_string(),
        password_hash: "".to_string(),
        role: "admin".to_string(),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    let config1 = AuthConfig {
        jwt_secret: "secret1".to_string(),
        token_expiry_hours: 24,
    };

    let config2 = AuthConfig {
        jwt_secret: "secret2".to_string(),
        token_expiry_hours: 24,
    };

    let token = generate_token(&user, &config1).unwrap();

    // Verification with different secret should fail
    assert!(verify_token(&token, &config2).is_err());
}
