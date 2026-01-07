//! HFT Server - Web API and WebSocket server for HFT Supervisor
//!
//! This crate provides:
//! - REST API for managing bots, configs, and data
//! - WebSocket streaming for real-time status updates
//! - Authentication via JWT
//! - IPC communication with hft-supervisor

pub mod api;
pub mod auth;
pub mod db;
pub mod state;
pub mod supervisor;
pub mod ws;

pub use state::{AppState, ServerConfig};
pub use supervisor::SupervisorClient;
