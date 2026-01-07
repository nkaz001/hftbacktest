//! WebSocket event streaming

use std::time::Duration;

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::Response,
};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};

use crate::state::AppState;
use crate::supervisor::ClusterStatus;

/// WebSocket event types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsEvent {
    /// Cluster status update
    StatusUpdate(ClusterStatus),
    /// Bot status changed
    BotStatusChanged {
        bot_id: String,
        old_state: String,
        new_state: String,
    },
    /// Bot started
    BotStarted { bot_id: String },
    /// Bot stopped
    BotStopped { bot_id: String },
    /// Config changed
    ConfigChanged { config_type: String },
    /// Error event
    Error { message: String },
    /// Ping for keepalive
    Ping,
    /// Pong response
    Pong,
}

/// WebSocket upgrade handler
pub async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> Response {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

/// Handle WebSocket connection
async fn handle_socket(socket: WebSocket, state: AppState) {
    let (mut sender, mut receiver) = socket.split();

    // Subscribe to events
    let mut event_rx = state.event_tx.subscribe();

    // Spawn task to forward events to client
    let send_task = tokio::spawn(async move {
        loop {
            tokio::select! {
                // Forward broadcast events
                result = event_rx.recv() => {
                    match result {
                        Ok(event) => {
                            let msg = serde_json::to_string(&event).unwrap_or_default();
                            if sender.send(Message::Text(msg.into())).await.is_err() {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
                // Send periodic ping
                _ = tokio::time::sleep(Duration::from_secs(30)) => {
                    let ping = serde_json::to_string(&WsEvent::Ping).unwrap_or_default();
                    if sender.send(Message::Text(ping.into())).await.is_err() {
                        break;
                    }
                }
            }
        }
    });

    // Handle incoming messages
    while let Some(result) = receiver.next().await {
        match result {
            Ok(Message::Text(text)) => {
                // Parse client message
                if let Ok(event) = serde_json::from_str::<WsEvent>(&text) {
                    match event {
                        WsEvent::Ping => {
                            // Client sent ping, respond with pong
                            let _ = state.event_tx.send(WsEvent::Pong);
                        }
                        _ => {
                            // Other messages are handled as needed
                            tracing::debug!(?event, "Received WebSocket message");
                        }
                    }
                }
            }
            Ok(Message::Close(_)) => {
                break;
            }
            Err(e) => {
                tracing::warn!(error = %e, "WebSocket error");
                break;
            }
            _ => {}
        }
    }

    // Clean up
    send_task.abort();
}

/// Background task to poll supervisor and broadcast status updates
pub async fn status_broadcaster(state: AppState) {
    let mut interval = tokio::time::interval(Duration::from_secs(5));
    let mut last_status: Option<ClusterStatus> = None;

    loop {
        interval.tick().await;

        if !state.supervisor.is_available().await {
            continue;
        }

        match state.supervisor.get_status().await {
            Ok(status) => {
                // Check for bot state changes
                if let Some(ref last) = last_status {
                    for new_bot in &status.bots {
                        if let Some(old_bot) = last.bots.iter().find(|b| b.id == new_bot.id) {
                            if old_bot.state != new_bot.state {
                                let _ = state.event_tx.send(WsEvent::BotStatusChanged {
                                    bot_id: new_bot.id.clone(),
                                    old_state: old_bot.state.clone(),
                                    new_state: new_bot.state.clone(),
                                });
                            }
                        }
                    }
                }

                // Broadcast full status
                let _ = state.event_tx.send(WsEvent::StatusUpdate(status.clone()));
                last_status = Some(status);
            }
            Err(e) => {
                tracing::warn!(error = %e, "Failed to get supervisor status");
            }
        }
    }
}
