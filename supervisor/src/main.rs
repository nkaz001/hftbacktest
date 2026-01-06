//! HFT Supervisor - Bot process lifecycle management
//!
//! This is the main entry point for the hft-supervisor daemon.
//! It manages multiple trading bot processes, handles configuration reloading,
//! and provides a control interface via Unix socket.

mod config;
mod health;
mod ipc;
mod manager;
mod warmup;

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Parser;
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::signal::unix::{signal, SignalKind};
use tokio::sync::mpsc;
use tracing::{error, info, warn};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use crate::config::ClusterConfig;
use crate::health::HealthChecker;
use crate::ipc::ControlServer;
use crate::manager::BotManager;

#[derive(Parser)]
#[command(name = "hft-supervisor")]
#[command(about = "HFT Bot Supervisor - Process lifecycle management for trading bots")]
struct Cli {
    /// Path to cluster configuration file
    #[arg(short, long, default_value = "/etc/hft/cluster.toml")]
    config: PathBuf,

    /// Directory containing strategy binaries
    #[arg(long, default_value = "/opt/hft/bin")]
    bin_dir: PathBuf,

    /// Enable JSON logging
    #[arg(long)]
    json_log: bool,

    /// Start all enabled bots on startup
    #[arg(long, default_value = "true")]
    start_on_boot: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize logging
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    if cli.json_log {
        tracing_subscriber::registry()
            .with(filter)
            .with(fmt::layer().json())
            .init();
    } else {
        tracing_subscriber::registry()
            .with(filter)
            .with(fmt::layer())
            .init();
    }

    info!("HFT Supervisor starting...");
    info!(config = %cli.config.display(), "Loading configuration");

    // Load configuration
    let config = ClusterConfig::load(&cli.config).context("Failed to load cluster configuration")?;

    info!(
        cluster = config.cluster.name,
        bots = config.bots.len(),
        "Configuration loaded"
    );

    // Create manager
    let manager = Arc::new(BotManager::new(config.clone(), cli.bin_dir.clone()));

    // Create health checker
    let health_checker = Arc::new(HealthChecker::new(config.warmup.data_dir.clone()));

    // Create control server
    let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>(1);
    let control_server = ControlServer::new(
        config.cluster.socket_path.clone(),
        manager.clone(),
        health_checker.clone(),
        config.cluster.name.clone(),
    );

    // Spawn control server
    let control_handle = tokio::spawn(async move {
        if let Err(e) = control_server.run(shutdown_rx).await {
            error!(error = %e, "Control server error");
        }
    });

    // Set up config file watcher
    let (config_tx, mut config_rx) = mpsc::channel::<PathBuf>(16);
    let config_path = cli.config.clone();
    let config_path_for_watcher = config_path.clone();

    let mut watcher: RecommendedWatcher = notify::recommended_watcher(
        move |res: Result<Event, notify::Error>| match res {
            Ok(event) => {
                if event.kind.is_modify() {
                    for path in event.paths {
                        let _ = config_tx.blocking_send(path);
                    }
                }
            }
            Err(e) => {
                warn!(error = %e, "File watcher error");
            }
        },
    )?;

    watcher.watch(&config_path_for_watcher, RecursiveMode::NonRecursive)?;
    info!(path = %config_path.display(), "Watching configuration file for changes");

    // Subscribe to manager events
    let mut events_rx = manager.subscribe();
    let manager_for_events = manager.clone();
    tokio::spawn(async move {
        while let Ok(event) = events_rx.recv().await {
            info!(?event, "Manager event");
        }
    });

    // Start monitoring task
    let manager_for_monitor = manager.clone();
    tokio::spawn(async move {
        manager_for_monitor.monitor().await;
    });

    // Start all enabled bots if configured
    if cli.start_on_boot {
        info!("Starting all enabled bots...");
        if let Err(e) = manager.start_all().await {
            error!(error = %e, "Failed to start bots");
        }
    }

    // Set up signal handlers
    let mut sigterm = signal(SignalKind::terminate())?;
    let mut sigint = signal(SignalKind::interrupt())?;
    let mut sighup = signal(SignalKind::hangup())?;

    // Notify systemd that we're ready (if running under systemd)
    #[cfg(feature = "systemd")]
    {
        let _ = sd_notify::notify(true, &[sd_notify::NotifyState::Ready]);
    }

    info!("HFT Supervisor ready");

    // Main event loop
    loop {
        tokio::select! {
            // Handle SIGTERM/SIGINT - graceful shutdown
            _ = sigterm.recv() => {
                info!("Received SIGTERM, initiating shutdown...");
                break;
            }
            _ = sigint.recv() => {
                info!("Received SIGINT, initiating shutdown...");
                break;
            }

            // Handle SIGHUP - reload configuration
            _ = sighup.recv() => {
                info!("Received SIGHUP, reloading configuration...");
                match ClusterConfig::load(&cli.config) {
                    Ok(new_config) => {
                        if let Err(e) = manager.reload_config(new_config).await {
                            error!(error = %e, "Failed to reload configuration");
                        } else {
                            info!("Configuration reloaded successfully");
                        }
                    }
                    Err(e) => {
                        error!(error = %e, "Failed to parse new configuration");
                    }
                }
            }

            // Handle config file changes from watcher
            Some(changed_path) = config_rx.recv() => {
                if changed_path == cli.config {
                    info!("Configuration file changed, reloading...");
                    // Wait a bit for file to be fully written
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

                    match ClusterConfig::load(&cli.config) {
                        Ok(new_config) => {
                            if let Err(e) = manager.reload_config(new_config).await {
                                error!(error = %e, "Failed to reload configuration");
                            } else {
                                info!("Configuration reloaded successfully");
                            }
                        }
                        Err(e) => {
                            error!(error = %e, "Failed to parse new configuration");
                        }
                    }
                }
            }
        }
    }

    // Graceful shutdown
    info!("Stopping all bots...");
    if let Err(e) = manager.stop_all().await {
        error!(error = %e, "Error stopping bots");
    }

    // Shutdown control server
    let _ = shutdown_tx.send(()).await;
    let _ = control_handle.await;

    info!("HFT Supervisor shutdown complete");
    Ok(())
}
