//! SQLite database layer for persisting state and audit logs

use std::path::Path;

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{
    sqlite::{SqliteConnectOptions, SqlitePool},
    FromRow, Pool, Sqlite,
};

/// Database connection pool
#[derive(Clone)]
pub struct Database {
    pool: Pool<Sqlite>,
}

impl Database {
    /// Create a new database connection
    ///
    /// Accepts URLs like:
    /// - `sqlite:path/to/db.sqlite`
    /// - `sqlite://path/to/db.sqlite`
    /// - `path/to/db.sqlite` (will be prefixed with sqlite:)
    pub async fn new(database_url: &str) -> Result<Self> {
        // Parse or construct the connection options
        let options = if database_url.starts_with("sqlite:") {
            database_url
                .parse::<SqliteConnectOptions>()?
                .create_if_missing(true)
        } else {
            SqliteConnectOptions::new()
                .filename(database_url)
                .create_if_missing(true)
        };

        // Ensure parent directory exists
        let filename = options.clone().get_filename();
        if let Some(parent) = filename.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }

        let pool = SqlitePool::connect_with(options).await?;

        // Run migrations
        sqlx::migrate!("./migrations").run(&pool).await?;

        let db = Self { pool };

        // Create default admin user if no users exist
        let user_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
            .fetch_one(db.pool())
            .await?;

        if user_count.0 == 0 {
            use crate::auth::hash_password;
            let admin_hash = hash_password("admin")?;
            db.create_user("admin", &admin_hash, "admin").await?;
            tracing::info!("Created default admin user (password: admin)");
        }

        Ok(db)
    }

    /// Create an in-memory database (for testing)
    pub async fn in_memory() -> Result<Self> {
        let pool = SqlitePool::connect(":memory:").await?;
        sqlx::migrate!("./migrations").run(&pool).await?;

        let db = Self { pool };

        // Create default admin user with properly hashed password
        use crate::auth::hash_password;
        let admin_hash = hash_password("admin")?;
        db.create_user("admin", &admin_hash, "admin").await?;

        Ok(db)
    }

    /// Get a reference to the connection pool
    pub fn pool(&self) -> &Pool<Sqlite> {
        &self.pool
    }
}

/// User record
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct User {
    pub id: i64,
    pub username: String,
    pub password_hash: String,
    pub role: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// User repository
impl Database {
    /// Find user by username
    pub async fn find_user_by_username(&self, username: &str) -> Result<Option<User>> {
        let user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE username = ?")
            .bind(username)
            .fetch_optional(&self.pool)
            .await?;
        Ok(user)
    }

    /// Find user by ID
    pub async fn find_user_by_id(&self, id: i64) -> Result<Option<User>> {
        let user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(user)
    }

    /// Create a new user
    pub async fn create_user(
        &self,
        username: &str,
        password_hash: &str,
        role: &str,
    ) -> Result<User> {
        let now = Utc::now();
        let result = sqlx::query(
            "INSERT INTO users (username, password_hash, role, created_at, updated_at) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(username)
        .bind(password_hash)
        .bind(role)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await?;

        let user = self
            .find_user_by_id(result.last_insert_rowid())
            .await?
            .expect("User was just created");

        Ok(user)
    }
}

/// Audit log entry
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct AuditLog {
    pub id: i64,
    pub user_id: Option<i64>,
    pub action: String,
    pub resource_type: String,
    pub resource_id: Option<String>,
    pub details: Option<String>,
    pub ip_address: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Audit log repository
impl Database {
    /// Log an action
    pub async fn log_action(
        &self,
        user_id: Option<i64>,
        action: &str,
        resource_type: &str,
        resource_id: Option<&str>,
        details: Option<&str>,
        ip_address: Option<&str>,
    ) -> Result<()> {
        let now = Utc::now();
        sqlx::query(
            "INSERT INTO audit_logs (user_id, action, resource_type, resource_id, details, ip_address, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(user_id)
        .bind(action)
        .bind(resource_type)
        .bind(resource_id)
        .bind(details)
        .bind(ip_address)
        .bind(now)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Get recent audit logs
    pub async fn get_audit_logs(&self, limit: i64, offset: i64) -> Result<Vec<AuditLog>> {
        let logs = sqlx::query_as::<_, AuditLog>(
            "SELECT * FROM audit_logs ORDER BY created_at DESC LIMIT ? OFFSET ?",
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;

        Ok(logs)
    }

    /// Get audit logs for a specific resource
    pub async fn get_audit_logs_for_resource(
        &self,
        resource_type: &str,
        resource_id: &str,
    ) -> Result<Vec<AuditLog>> {
        let logs = sqlx::query_as::<_, AuditLog>(
            "SELECT * FROM audit_logs WHERE resource_type = ? AND resource_id = ? ORDER BY created_at DESC",
        )
        .bind(resource_type)
        .bind(resource_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(logs)
    }
}

/// Bot state history
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct BotStateHistory {
    pub id: i64,
    pub bot_id: String,
    pub state: String,
    pub previous_state: Option<String>,
    pub triggered_by: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Bot state history repository
impl Database {
    /// Record a bot state change
    pub async fn record_bot_state_change(
        &self,
        bot_id: &str,
        state: &str,
        previous_state: Option<&str>,
        triggered_by: Option<&str>,
    ) -> Result<()> {
        let now = Utc::now();
        sqlx::query(
            "INSERT INTO bot_state_history (bot_id, state, previous_state, triggered_by, created_at) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(bot_id)
        .bind(state)
        .bind(previous_state)
        .bind(triggered_by)
        .bind(now)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Get bot state history
    pub async fn get_bot_state_history(
        &self,
        bot_id: &str,
        limit: i64,
    ) -> Result<Vec<BotStateHistory>> {
        let history = sqlx::query_as::<_, BotStateHistory>(
            "SELECT * FROM bot_state_history WHERE bot_id = ? ORDER BY created_at DESC LIMIT ?",
        )
        .bind(bot_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(history)
    }
}

/// Session record for tracking login sessions
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Session {
    pub id: i64,
    pub user_id: i64,
    pub token_hash: String,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

/// Session repository
impl Database {
    /// Create a new session
    pub async fn create_session(
        &self,
        user_id: i64,
        token_hash: &str,
        expires_at: DateTime<Utc>,
    ) -> Result<Session> {
        let now = Utc::now();
        let result = sqlx::query(
            "INSERT INTO sessions (user_id, token_hash, expires_at, created_at) VALUES (?, ?, ?, ?)",
        )
        .bind(user_id)
        .bind(token_hash)
        .bind(expires_at)
        .bind(now)
        .execute(&self.pool)
        .await?;

        let session = sqlx::query_as::<_, Session>("SELECT * FROM sessions WHERE id = ?")
            .bind(result.last_insert_rowid())
            .fetch_one(&self.pool)
            .await?;

        Ok(session)
    }

    /// Find session by token hash
    pub async fn find_session_by_token(&self, token_hash: &str) -> Result<Option<Session>> {
        let session = sqlx::query_as::<_, Session>(
            "SELECT * FROM sessions WHERE token_hash = ? AND expires_at > ?",
        )
        .bind(token_hash)
        .bind(Utc::now())
        .fetch_optional(&self.pool)
        .await?;

        Ok(session)
    }

    /// Delete session
    pub async fn delete_session(&self, token_hash: &str) -> Result<()> {
        sqlx::query("DELETE FROM sessions WHERE token_hash = ?")
            .bind(token_hash)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    /// Delete all sessions for a user
    pub async fn delete_user_sessions(&self, user_id: i64) -> Result<()> {
        sqlx::query("DELETE FROM sessions WHERE user_id = ?")
            .bind(user_id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    /// Clean up expired sessions
    pub async fn cleanup_expired_sessions(&self) -> Result<u64> {
        let result = sqlx::query("DELETE FROM sessions WHERE expires_at < ?")
            .bind(Utc::now())
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected())
    }
}
