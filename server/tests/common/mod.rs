//! Common test utilities

use axum::{
    routing::{get, post, put},
    Router,
};
use hft_server::api::{bots, config, data, logs, status};
use hft_server::auth;
use hft_server::state::{AppState, ServerConfig};
use hft_server::ws;
use std::path::PathBuf;
use tower_http::cors::{Any, CorsLayer};

/// Create a test app with in-memory state
pub async fn create_test_app() -> Router {
    let config = ServerConfig {
        cluster_config_path: PathBuf::from("/tmp/nonexistent-cluster.toml"),
        data_dir: PathBuf::from("/tmp/nonexistent-data"),
        log_dir: PathBuf::from("/tmp/nonexistent-logs"),
        supervisor_socket: PathBuf::from("/tmp/nonexistent-supervisor.sock"),
        jwt_secret: "test-secret-key".to_string(),
        bind_addr: "127.0.0.1".to_string(),
        port: 3000,
        database_url: None,
    };

    let state = AppState::new(config);

    Router::new()
        // Health check
        .route("/api/health", get(status::health_check))
        // Auth endpoints
        .route("/api/auth/login", post(auth::login))
        .route("/api/auth/me", get(auth::get_current_user))
        .route("/api/auth/logout", post(auth::logout))
        // Status
        .route("/api/status", get(status::get_status))
        // Bots
        .route("/api/bots", get(bots::list_bots))
        .route("/api/bots/:id", get(bots::get_bot))
        .route("/api/bots/:id/start", post(bots::start_bot))
        .route("/api/bots/:id/stop", post(bots::stop_bot))
        .route("/api/bots/:id/restart", post(bots::restart_bot))
        .route("/api/bots/restart-all", post(bots::restart_all_bots))
        // Config
        .route("/api/config/cluster", get(config::get_cluster_config))
        .route("/api/config/cluster", put(config::update_cluster_config))
        .route("/api/config/strategies", get(config::list_strategy_configs))
        .route(
            "/api/config/strategies/:bot_id",
            get(config::get_strategy_config),
        )
        .route(
            "/api/config/strategies/:bot_id",
            put(config::update_strategy_config),
        )
        // Data
        .route("/api/data/files", get(data::list_data_files))
        .route("/api/data/stats", get(data::get_data_stats))
        .route("/api/data/warmup/:symbol", get(data::get_warmup_status))
        // Logs
        .route("/api/logs/files", get(logs::list_log_files))
        .route("/api/logs/:component", get(logs::read_logs))
        .route("/api/logs/:component/tail", get(logs::tail_logs))
        // WebSocket
        .route("/ws", get(ws::ws_handler))
        // CORS
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .with_state(state)
}

/// Create test app with database
pub async fn create_test_app_with_db() -> Router {
    use hft_server::db::Database;

    let config = ServerConfig {
        cluster_config_path: PathBuf::from("/tmp/nonexistent-cluster.toml"),
        data_dir: PathBuf::from("/tmp/nonexistent-data"),
        log_dir: PathBuf::from("/tmp/nonexistent-logs"),
        supervisor_socket: PathBuf::from("/tmp/nonexistent-supervisor.sock"),
        jwt_secret: "test-secret-key".to_string(),
        bind_addr: "127.0.0.1".to_string(),
        port: 3000,
        database_url: Some("sqlite::memory:".to_string()),
    };

    // Create in-memory database
    let db = Database::in_memory().await.expect("Failed to create test database");

    let state = AppState::new(config).with_db(db);

    Router::new()
        .route("/api/health", get(status::health_check))
        .route("/api/auth/login", post(auth::login))
        .route("/api/auth/me", get(auth::get_current_user))
        .route("/api/auth/logout", post(auth::logout))
        .route("/api/status", get(status::get_status))
        .route("/api/bots", get(bots::list_bots))
        .route("/api/bots/:id", get(bots::get_bot))
        .route("/api/bots/:id/start", post(bots::start_bot))
        .route("/api/bots/:id/stop", post(bots::stop_bot))
        .route("/api/bots/:id/restart", post(bots::restart_bot))
        .route("/api/bots/restart-all", post(bots::restart_all_bots))
        .route("/api/config/cluster", get(config::get_cluster_config))
        .route("/api/config/cluster", put(config::update_cluster_config))
        .route("/api/config/strategies", get(config::list_strategy_configs))
        .route(
            "/api/config/strategies/:bot_id",
            get(config::get_strategy_config),
        )
        .route(
            "/api/config/strategies/:bot_id",
            put(config::update_strategy_config),
        )
        .route("/api/data/files", get(data::list_data_files))
        .route("/api/data/stats", get(data::get_data_stats))
        .route("/api/data/warmup/:symbol", get(data::get_warmup_status))
        .route("/api/logs/files", get(logs::list_log_files))
        .route("/api/logs/:component", get(logs::read_logs))
        .route("/api/logs/:component/tail", get(logs::tail_logs))
        .route("/ws", get(ws::ws_handler))
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .with_state(state)
}
