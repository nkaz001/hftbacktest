//! JWT authentication module

use anyhow::Result;
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use axum::{
    async_trait,
    extract::{FromRequestParts, State},
    http::{header, request::Parts, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

use crate::db::User;
use crate::state::AppState;

/// JWT claims
#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    /// Subject (user ID)
    pub sub: i64,
    /// Username
    pub username: String,
    /// Role
    pub role: String,
    /// Expiration time (as Unix timestamp)
    pub exp: i64,
    /// Issued at (as Unix timestamp)
    pub iat: i64,
}

/// Auth configuration
#[derive(Clone)]
pub struct AuthConfig {
    pub jwt_secret: String,
    pub token_expiry_hours: i64,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            jwt_secret: "change-me-in-production".to_string(),
            token_expiry_hours: 24,
        }
    }
}

/// Generate a JWT token for a user
pub fn generate_token(user: &User, config: &AuthConfig) -> Result<String> {
    let now = Utc::now();
    let exp = now + Duration::hours(config.token_expiry_hours);

    let claims = Claims {
        sub: user.id,
        username: user.username.clone(),
        role: user.role.clone(),
        exp: exp.timestamp(),
        iat: now.timestamp(),
    };

    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(config.jwt_secret.as_bytes()),
    )?;

    Ok(token)
}

/// Verify a JWT token and return claims
pub fn verify_token(token: &str, config: &AuthConfig) -> Result<Claims> {
    let token_data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(config.jwt_secret.as_bytes()),
        &Validation::default(),
    )?;

    Ok(token_data.claims)
}

/// Hash a password using Argon2
pub fn hash_password(password: &str) -> Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let password_hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| anyhow::anyhow!("Failed to hash password: {}", e))?
        .to_string();

    Ok(password_hash)
}

/// Verify a password against a hash
pub fn verify_password(password: &str, password_hash: &str) -> Result<bool> {
    let parsed_hash =
        PasswordHash::new(password_hash).map_err(|e| anyhow::anyhow!("Invalid hash: {}", e))?;

    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok())
}

/// Authenticated user extracted from JWT
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user_id: i64,
    pub username: String,
    pub role: String,
}

/// Auth error for failed authentication
pub struct AuthError {
    pub message: String,
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({
                "error": "unauthorized",
                "message": self.message
            })),
        )
            .into_response()
    }
}

/// Extractor for authenticated user
#[async_trait]
impl FromRequestParts<AppState> for AuthUser {
    type Rejection = AuthError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        // Get Authorization header
        let auth_header = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .ok_or_else(|| AuthError {
                message: "Missing authorization header".to_string(),
            })?;

        // Extract Bearer token
        let token = auth_header
            .strip_prefix("Bearer ")
            .ok_or_else(|| AuthError {
                message: "Invalid authorization header format".to_string(),
            })?;

        // Verify token
        let auth_config = AuthConfig {
            jwt_secret: state.config.jwt_secret.clone(),
            ..Default::default()
        };

        let claims = verify_token(token, &auth_config).map_err(|e| AuthError {
            message: format!("Invalid token: {}", e),
        })?;

        Ok(AuthUser {
            user_id: claims.sub,
            username: claims.username,
            role: claims.role,
        })
    }
}

/// Login request
#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

/// Login response
#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub expires_at: String,
    pub user: UserInfo,
}

#[derive(Debug, Serialize)]
pub struct UserInfo {
    pub id: i64,
    pub username: String,
    pub role: String,
}

/// POST /api/auth/login
pub async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, AuthError> {
    // Find user
    let db = state.db.as_ref().ok_or_else(|| AuthError {
        message: "Database not configured".to_string(),
    })?;

    let user = db
        .find_user_by_username(&req.username)
        .await
        .map_err(|e| AuthError {
            message: format!("Database error: {}", e),
        })?
        .ok_or_else(|| AuthError {
            message: "Invalid username or password".to_string(),
        })?;

    // Verify password
    let valid = verify_password(&req.password, &user.password_hash).map_err(|e| AuthError {
        message: format!("Password verification error: {}", e),
    })?;

    if !valid {
        return Err(AuthError {
            message: "Invalid username or password".to_string(),
        });
    }

    // Generate token
    let auth_config = AuthConfig {
        jwt_secret: state.config.jwt_secret.clone(),
        ..Default::default()
    };

    let token = generate_token(&user, &auth_config).map_err(|e| AuthError {
        message: format!("Token generation error: {}", e),
    })?;

    let expires_at = Utc::now() + Duration::hours(auth_config.token_expiry_hours);

    // Log the login
    if let Some(ref db) = state.db {
        let _ = db
            .log_action(
                Some(user.id),
                "login",
                "user",
                Some(&user.id.to_string()),
                None,
                None,
            )
            .await;
    }

    Ok(Json(LoginResponse {
        token,
        expires_at: expires_at.to_rfc3339(),
        user: UserInfo {
            id: user.id,
            username: user.username,
            role: user.role,
        },
    }))
}

/// GET /api/auth/me - Get current user info
pub async fn get_current_user(auth_user: AuthUser) -> Json<UserInfo> {
    Json(UserInfo {
        id: auth_user.user_id,
        username: auth_user.username,
        role: auth_user.role,
    })
}

/// POST /api/auth/logout
pub async fn logout(
    State(state): State<AppState>,
    auth_user: AuthUser,
) -> Result<Json<serde_json::Value>, AuthError> {
    // Log the logout
    if let Some(ref db) = state.db {
        let _ = db
            .log_action(
                Some(auth_user.user_id),
                "logout",
                "user",
                Some(&auth_user.user_id.to_string()),
                None,
                None,
            )
            .await;
    }

    Ok(Json(serde_json::json!({
        "success": true,
        "message": "Logged out successfully"
    })))
}
