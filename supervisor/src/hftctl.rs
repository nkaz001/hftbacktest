//! hftctl - HFT Supervisor Control Tool
//!
//! Command-line interface for controlling the HFT supervisor.

mod config;
mod health;
mod infra;
mod ipc;
mod manager;
mod warmup;

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

use crate::ipc::{ControlClient, ControlRequest, ControlResponse, StatusResponse};

#[derive(Parser)]
#[command(name = "hftctl")]
#[command(about = "HFT Supervisor Control Tool")]
struct Cli {
    /// Path to supervisor socket
    #[arg(short, long, default_value = "/run/hft/supervisor.sock")]
    socket: PathBuf,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Show cluster status
    Status {
        /// Show specific bot status
        #[arg(short, long)]
        bot: Option<String>,
    },

    /// Start a bot
    Start {
        /// Bot ID to start
        bot_id: String,
    },

    /// Stop a bot
    Stop {
        /// Bot ID to stop
        bot_id: String,
    },

    /// Restart a bot
    Restart {
        /// Bot ID to restart
        bot_id: String,
    },

    /// Restart all bots
    RestartAll,

    /// Reload configuration
    Reload,

    /// Check warmup data status
    WarmupStatus {
        /// Symbol to check (optional, checks all if not specified)
        symbol: Option<String>,
    },

    /// Validate configuration file
    Validate {
        /// Path to configuration file
        #[arg(default_value = "/etc/hft/cluster.toml")]
        config: PathBuf,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Validate { config } => {
            // Validate configuration without connecting to supervisor
            match crate::config::ClusterConfig::load(&config) {
                Ok(cfg) => {
                    println!("Configuration valid!");
                    println!("  Cluster: {}", cfg.cluster.name);
                    // Show all connectors
                    let connectors = cfg.all_connectors();
                    println!("  Connectors: {}", connectors.len());
                    for connector in &connectors {
                        println!("    - {} ({})", connector.name, connector.connector_type);
                    }
                    println!(
                        "  Collector: {}",
                        if cfg.collector.enabled {
                            "enabled"
                        } else {
                            "disabled"
                        }
                    );
                    println!(
                        "  Bots: {} total, {} enabled",
                        cfg.bots.len(),
                        cfg.bots.iter().filter(|b| b.enabled).count()
                    );
                    for bot in &cfg.bots {
                        let status = if bot.enabled { "enabled" } else { "disabled" };
                        let connector_info = bot.connector.as_deref().unwrap_or("default");
                        println!(
                            "    - {} ({}) [{}] -> {}",
                            bot.id, bot.symbol, status, connector_info
                        );
                    }
                    Ok(())
                }
                Err(e) => {
                    eprintln!("Configuration invalid: {}", e);
                    std::process::exit(1);
                }
            }
        }

        // All other commands require connection to supervisor
        cmd => {
            let client = ControlClient::new(&cli.socket);

            let request = match cmd {
                Commands::Status { bot } => {
                    if let Some(bot_id) = bot {
                        ControlRequest::BotStatus { bot_id }
                    } else {
                        ControlRequest::Status
                    }
                }
                Commands::Start { bot_id } => ControlRequest::StartBot { bot_id },
                Commands::Stop { bot_id } => ControlRequest::StopBot { bot_id },
                Commands::Restart { bot_id } => ControlRequest::RestartBot { bot_id },
                Commands::RestartAll => ControlRequest::RestartAll,
                Commands::Reload => ControlRequest::Reload,
                Commands::WarmupStatus { symbol } => ControlRequest::WarmupStatus { symbol },
                Commands::Validate { .. } => unreachable!(),
            };

            let response = client
                .send(request)
                .await
                .context("Failed to communicate with supervisor")?;

            match response {
                ControlResponse::Ok { data } => {
                    if let Some(data) = data {
                        // Format output based on data type
                        if let Ok(status) = serde_json::from_value::<StatusResponse>(data.clone()) {
                            print_status(&status);
                        } else {
                            // Pretty print JSON
                            println!("{}", serde_json::to_string_pretty(&data)?);
                        }
                    } else {
                        println!("OK");
                    }
                    Ok(())
                }
                ControlResponse::Error { message } => {
                    eprintln!("Error: {}", message);
                    std::process::exit(1);
                }
            }
        }
    }
}

fn print_status(status: &StatusResponse) {
    println!(
        "Cluster: {} (uptime: {})",
        status.cluster_name,
        format_duration(status.uptime_secs)
    );
    println!();

    // Connectors status
    println!("Connectors:");
    if status.connectors.is_empty() {
        println!("  No connectors configured");
    } else {
        for connector in &status.connectors {
            let icon = if connector.healthy { "●" } else { "○" };
            let color = if connector.healthy {
                "\x1b[32m"
            } else {
                "\x1b[31m"
            };
            println!(
                "{}{}  {}: {}\x1b[0m",
                color, icon, connector.name, connector.status
            );
        }
    }

    // Collector status
    let collector_icon = if status.collector.healthy {
        "●"
    } else {
        "○"
    };
    let collector_color = if status.collector.healthy {
        "\x1b[32m"
    } else {
        "\x1b[31m"
    };
    println!(
        "\n{}{}  Collector: {}\x1b[0m",
        collector_color, collector_icon, status.collector.status
    );

    println!();
    println!("Bots:");

    if status.bots.is_empty() {
        println!("  No bots configured");
    } else {
        for bot in &status.bots {
            let (icon, color) = match bot.state.as_str() {
                "running" => ("●", "\x1b[32m"),
                "starting" | "waiting_warmup" => ("◐", "\x1b[33m"),
                "stopped" => ("○", "\x1b[90m"),
                "crashed" | "failed" => ("✗", "\x1b[31m"),
                _ => ("?", "\x1b[90m"),
            };

            let uptime_str = bot
                .uptime_secs
                .map(|s| format!(" ({})", format_duration(s)))
                .unwrap_or_default();

            let pid_str = bot.pid.map(|p| format!(" [pid {}]", p)).unwrap_or_default();

            println!(
                "  {}{}  {}: {}{}{}\x1b[0m",
                color, icon, bot.id, bot.state, uptime_str, pid_str
            );
        }
    }
}

fn format_duration(secs: u64) -> String {
    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        format!("{}m {}s", secs / 60, secs % 60)
    } else if secs < 86400 {
        format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
    } else {
        format!("{}d {}h", secs / 86400, (secs % 86400) / 3600)
    }
}
