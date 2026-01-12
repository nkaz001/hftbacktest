//! Bot process lifecycle management
//!
//! Handles starting, stopping, and monitoring bot processes.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use nix::sys::signal::{self, Signal};
use nix::unistd::Pid;
use tokio::process::{Child, Command};
use tokio::sync::{broadcast, RwLock};
use tracing::{error, info, warn};

use crate::config::{BotConfig, BotDefaults, ClusterConfig};
use crate::warmup::WarmupChecker;

/// Bot process state
#[derive(Debug, Clone, PartialEq)]
pub enum BotState {
    /// Bot is stopped
    Stopped,
    /// Bot is waiting for warmup data
    WaitingWarmup,
    /// Bot is starting up (warmup in progress)
    Starting,
    /// Bot is running normally
    Running,
    /// Bot crashed and is waiting to restart
    Crashed {
        exit_code: Option<i32>,
        restart_at: Instant,
        restart_count: u32,
    },
    /// Bot has been disabled due to too many crashes
    Failed { reason: String },
}

impl BotState {
    pub fn as_str(&self) -> &'static str {
        match self {
            BotState::Stopped => "stopped",
            BotState::WaitingWarmup => "waiting_warmup",
            BotState::Starting => "starting",
            BotState::Running => "running",
            BotState::Crashed { .. } => "crashed",
            BotState::Failed { .. } => "failed",
        }
    }
}

/// Information about a managed bot
#[derive(Debug)]
pub struct ManagedBot {
    pub config: BotConfig,
    pub state: BotState,
    pub process: Option<Child>,
    pub pid: Option<u32>,
    pub started_at: Option<Instant>,
    pub restart_count: u32,
}

impl ManagedBot {
    pub fn new(config: BotConfig) -> Self {
        Self {
            config,
            state: BotState::Stopped,
            process: None,
            pid: None,
            started_at: None,
            restart_count: 0,
        }
    }
}

/// Bot info for status reporting (IPC-friendly)
#[derive(Debug, Clone)]
pub struct BotInfo {
    pub id: String,
    pub symbol: String,
    pub connector: Option<String>,
    pub state: String,
    pub pid: Option<u32>,
    pub uptime_secs: Option<u64>,
    pub restart_count: u32,
}

/// Events emitted by the manager
#[derive(Debug, Clone)]
pub enum ManagerEvent {
    BotStateChanged {
        bot_id: String,
        old_state: BotState,
        new_state: BotState,
    },
    BotStarted {
        bot_id: String,
        pid: u32,
    },
    BotStopped {
        bot_id: String,
        exit_code: Option<i32>,
    },
    BotCrashed {
        bot_id: String,
        exit_code: Option<i32>,
        will_restart: bool,
    },
    WarmupWaiting {
        bot_id: String,
        reason: String,
    },
}

/// Commands that can be sent to the manager
#[derive(Debug)]
pub enum ManagerCommand {
    StartBot { bot_id: String },
    StopBot { bot_id: String },
    RestartBot { bot_id: String },
    ReloadConfig { config: ClusterConfig },
    Shutdown,
}

/// Bot process manager
pub struct BotManager {
    /// Cluster configuration
    config: Arc<RwLock<ClusterConfig>>,
    /// Managed bots
    bots: Arc<RwLock<HashMap<String, ManagedBot>>>,
    /// Warmup checker
    warmup_checker: Arc<WarmupChecker>,
    /// Event broadcaster
    event_tx: broadcast::Sender<ManagerEvent>,
    /// Binary path for strategies
    bin_dir: PathBuf,
    /// Bot defaults
    defaults: BotDefaults,
}

impl BotManager {
    pub fn new(config: ClusterConfig, bin_dir: PathBuf) -> Self {
        let (event_tx, _) = broadcast::channel(256);

        let warmup_checker = Arc::new(WarmupChecker::new(
            config.warmup.data_dir.clone(),
            Duration::from_secs(config.warmup.max_age_seconds),
            Duration::from_secs(config.warmup.min_duration_seconds),
        ));

        let defaults = config.bot_defaults.clone();
        let mut bots = HashMap::new();

        for bot_config in &config.bots {
            bots.insert(bot_config.id.clone(), ManagedBot::new(bot_config.clone()));
        }

        Self {
            config: Arc::new(RwLock::new(config)),
            bots: Arc::new(RwLock::new(bots)),
            warmup_checker,
            event_tx,
            bin_dir,
            defaults,
        }
    }

    /// Subscribe to manager events
    pub fn subscribe(&self) -> broadcast::Receiver<ManagerEvent> {
        self.event_tx.subscribe()
    }

    /// Get bot states for status reporting
    pub async fn get_bot_states(&self) -> HashMap<String, BotState> {
        let bots = self.bots.read().await;
        bots.iter()
            .map(|(id, bot)| (id.clone(), bot.state.clone()))
            .collect()
    }

    /// Get full bot info for IPC status reporting
    pub async fn get_all_bot_info(&self) -> Vec<BotInfo> {
        let bots = self.bots.read().await;
        bots.iter()
            .map(|(id, bot)| {
                let uptime_secs = bot.started_at.map(|t| t.elapsed().as_secs());
                BotInfo {
                    id: id.clone(),
                    symbol: bot.config.symbol.clone(),
                    connector: bot.config.connector.clone(),
                    state: bot.state.as_str().to_string(),
                    pid: bot.pid,
                    uptime_secs,
                    restart_count: bot.restart_count,
                }
            })
            .collect()
    }

    /// Get single bot info by ID
    pub async fn get_bot_info(&self, bot_id: &str) -> Option<BotInfo> {
        let bots = self.bots.read().await;
        bots.get(bot_id).map(|bot| {
            let uptime_secs = bot.started_at.map(|t| t.elapsed().as_secs());
            BotInfo {
                id: bot_id.to_string(),
                symbol: bot.config.symbol.clone(),
                connector: bot.config.connector.clone(),
                state: bot.state.as_str().to_string(),
                pid: bot.pid,
                uptime_secs,
                restart_count: bot.restart_count,
            }
        })
    }

    /// Start a specific bot
    pub async fn start_bot(&self, bot_id: &str) -> Result<()> {
        let mut bots = self.bots.write().await;
        let bot = bots
            .get_mut(bot_id)
            .with_context(|| format!("Bot not found: {}", bot_id))?;

        if !bot.config.enabled {
            anyhow::bail!("Bot {} is disabled in configuration", bot_id);
        }

        match &bot.state {
            BotState::Running | BotState::Starting => {
                anyhow::bail!("Bot {} is already running or starting", bot_id);
            }
            // WaitingWarmup is allowed - this is called when warmup data becomes ready
            BotState::WaitingWarmup => {}
            _ => {}
        }

        // Check warmup data readiness
        let warmup_status = self.warmup_checker.check_symbol(&bot.config.symbol).await;

        if !warmup_status.ready {
            let old_state = std::mem::replace(&mut bot.state, BotState::WaitingWarmup);
            self.emit_event(ManagerEvent::BotStateChanged {
                bot_id: bot_id.to_string(),
                old_state,
                new_state: BotState::WaitingWarmup,
            });
            self.emit_event(ManagerEvent::WarmupWaiting {
                bot_id: bot_id.to_string(),
                reason: warmup_status.reason.clone(),
            });
            info!(
                bot_id = bot_id,
                reason = warmup_status.reason,
                "Bot waiting for warmup data"
            );
            return Ok(());
        }

        // Start the bot process
        self.spawn_bot_process(bot).await?;

        Ok(())
    }

    /// Stop a specific bot
    pub async fn stop_bot(&self, bot_id: &str) -> Result<()> {
        let mut bots = self.bots.write().await;
        let bot = bots
            .get_mut(bot_id)
            .with_context(|| format!("Bot not found: {}", bot_id))?;

        if let Some(pid) = bot.pid {
            info!(bot_id = bot_id, pid = pid, "Sending SIGTERM to bot");

            // Send SIGTERM
            if let Err(e) = signal::kill(Pid::from_raw(pid as i32), Signal::SIGTERM) {
                warn!(
                    bot_id = bot_id,
                    pid = pid,
                    error = %e,
                    "Failed to send SIGTERM"
                );
            }

            // Wait for graceful shutdown (up to 30 seconds)
            let shutdown_timeout = Duration::from_secs(30);
            let start = Instant::now();

            while start.elapsed() < shutdown_timeout {
                if let Some(ref mut process) = bot.process {
                    match process.try_wait() {
                        Ok(Some(status)) => {
                            info!(
                                bot_id = bot_id,
                                exit_code = status.code(),
                                "Bot exited gracefully"
                            );
                            break;
                        }
                        Ok(None) => {
                            tokio::time::sleep(Duration::from_millis(100)).await;
                        }
                        Err(e) => {
                            warn!(bot_id = bot_id, error = %e, "Error waiting for bot");
                            break;
                        }
                    }
                }
            }

            // Force kill if still running
            if let Some(ref mut process) = bot.process {
                if process.try_wait().ok().flatten().is_none() {
                    warn!(bot_id = bot_id, "Bot did not exit gracefully, sending SIGKILL");
                    if let Err(e) = signal::kill(Pid::from_raw(pid as i32), Signal::SIGKILL) {
                        error!(bot_id = bot_id, error = %e, "Failed to send SIGKILL");
                    }
                    let _ = process.wait().await;
                }
            }
        }

        let old_state = std::mem::replace(&mut bot.state, BotState::Stopped);
        bot.process = None;
        bot.pid = None;
        bot.started_at = None;

        self.emit_event(ManagerEvent::BotStopped {
            bot_id: bot_id.to_string(),
            exit_code: None,
        });
        self.emit_event(ManagerEvent::BotStateChanged {
            bot_id: bot_id.to_string(),
            old_state,
            new_state: BotState::Stopped,
        });

        Ok(())
    }

    /// Restart a specific bot
    pub async fn restart_bot(&self, bot_id: &str) -> Result<()> {
        self.stop_bot(bot_id).await?;
        tokio::time::sleep(Duration::from_millis(500)).await;
        self.start_bot(bot_id).await?;
        Ok(())
    }

    /// Start all enabled bots
    pub async fn start_all(&self) -> Result<()> {
        let bot_ids: Vec<String> = {
            let bots = self.bots.read().await;
            bots.iter()
                .filter(|(_, b)| b.config.enabled)
                .map(|(id, _)| id.clone())
                .collect()
        };

        for bot_id in bot_ids {
            if let Err(e) = self.start_bot(&bot_id).await {
                error!(bot_id = bot_id, error = %e, "Failed to start bot");
            }
        }

        Ok(())
    }

    /// Stop all bots
    pub async fn stop_all(&self) -> Result<()> {
        let bot_ids: Vec<String> = {
            let bots = self.bots.read().await;
            bots.iter()
                .filter(|(_, b)| matches!(b.state, BotState::Running | BotState::Starting))
                .map(|(id, _)| id.clone())
                .collect()
        };

        for bot_id in bot_ids {
            if let Err(e) = self.stop_bot(&bot_id).await {
                error!(bot_id = bot_id, error = %e, "Failed to stop bot");
            }
        }

        Ok(())
    }

    /// Reload configuration and restart changed bots
    pub async fn reload_config(&self, new_config: ClusterConfig) -> Result<()> {
        let old_config = self.config.read().await.clone();
        let changed_bots = old_config.changed_bots(&new_config);

        info!(changed = ?changed_bots, "Configuration changed, reloading bots");

        // Stop changed bots
        for bot_id in &changed_bots {
            if let Err(e) = self.stop_bot(bot_id).await {
                warn!(bot_id = bot_id, error = %e, "Failed to stop bot for reload");
            }
        }

        // Update configuration
        {
            let mut config = self.config.write().await;
            *config = new_config.clone();
        }

        // Update bot configs
        {
            let mut bots = self.bots.write().await;

            // Remove bots that no longer exist
            let current_ids: std::collections::HashSet<_> =
                new_config.bots.iter().map(|b| &b.id).collect();
            bots.retain(|id, _| current_ids.contains(id));

            // Add or update bots
            for bot_config in &new_config.bots {
                if let Some(bot) = bots.get_mut(&bot_config.id) {
                    bot.config = bot_config.clone();
                } else {
                    bots.insert(bot_config.id.clone(), ManagedBot::new(bot_config.clone()));
                }
            }
        }

        // Restart changed bots that are enabled
        for bot_id in &changed_bots {
            let enabled = {
                let bots = self.bots.read().await;
                bots.get(bot_id).map(|b| b.config.enabled).unwrap_or(false)
            };

            if enabled {
                if let Err(e) = self.start_bot(bot_id).await {
                    error!(bot_id = bot_id, error = %e, "Failed to restart bot after reload");
                }
            }
        }

        Ok(())
    }

    /// Monitor running bots and handle crashes
    pub async fn monitor(&self) {
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;

            let mut updates = Vec::new();

            {
                let mut bots = self.bots.write().await;
                for (bot_id, bot) in bots.iter_mut() {
                    // Check running bots for exit
                    if let BotState::Running = bot.state {
                        if let Some(ref mut process) = bot.process {
                            match process.try_wait() {
                                Ok(Some(status)) => {
                                    let exit_code = status.code();
                                    warn!(
                                        bot_id = bot_id,
                                        exit_code = exit_code,
                                        "Bot process exited"
                                    );

                                    let will_restart = bot.config.auto_restart
                                        && bot.restart_count < bot.config.max_restarts;

                                    if will_restart {
                                        let restart_at = Instant::now()
                                            + Duration::from_secs(bot.config.restart_backoff_secs);
                                        bot.state = BotState::Crashed {
                                            exit_code,
                                            restart_at,
                                            restart_count: bot.restart_count + 1,
                                        };
                                        bot.restart_count += 1;
                                    } else {
                                        bot.state = BotState::Failed {
                                            reason: format!(
                                                "Exceeded max restarts ({})",
                                                bot.config.max_restarts
                                            ),
                                        };
                                    }

                                    bot.process = None;
                                    bot.pid = None;

                                    updates.push(ManagerEvent::BotCrashed {
                                        bot_id: bot_id.clone(),
                                        exit_code,
                                        will_restart,
                                    });
                                }
                                Ok(None) => {} // Still running
                                Err(e) => {
                                    error!(bot_id = bot_id, error = %e, "Error checking bot status");
                                }
                            }
                        }
                    }

                    // Handle crashed bots that need restart
                    if let BotState::Crashed {
                        restart_at,
                        restart_count,
                        ..
                    } = &bot.state
                    {
                        if Instant::now() >= *restart_at {
                            info!(
                                bot_id = bot_id,
                                attempt = restart_count,
                                "Attempting to restart crashed bot"
                            );
                            updates.push(ManagerEvent::BotStateChanged {
                                bot_id: bot_id.clone(),
                                old_state: bot.state.clone(),
                                new_state: BotState::Starting,
                            });
                        }
                    }

                    // Check warmup waiting bots
                    if let BotState::WaitingWarmup = bot.state {
                        // Will be handled in the warmup check loop
                    }
                }
            }

            // Emit events
            for event in updates {
                self.emit_event(event);
            }

            // Handle restarts outside of lock
            self.handle_pending_restarts().await;
            self.check_warmup_waiting().await;
        }
    }

    /// Handle bots that need to be restarted
    async fn handle_pending_restarts(&self) {
        let bots_to_restart: Vec<String> = {
            let bots = self.bots.read().await;
            bots.iter()
                .filter(|(_, bot)| {
                    if let BotState::Crashed { restart_at, .. } = &bot.state {
                        Instant::now() >= *restart_at
                    } else {
                        false
                    }
                })
                .map(|(id, _)| id.clone())
                .collect()
        };

        for bot_id in bots_to_restart {
            // Update state to starting
            {
                let mut bots = self.bots.write().await;
                if let Some(bot) = bots.get_mut(&bot_id) {
                    bot.state = BotState::Starting;
                }
            }

            if let Err(e) = self.start_bot(&bot_id).await {
                error!(bot_id = bot_id, error = %e, "Failed to restart bot");
            }
        }
    }

    /// Check bots waiting for warmup data
    async fn check_warmup_waiting(&self) {
        let bots_waiting: Vec<(String, String)> = {
            let bots = self.bots.read().await;
            bots.iter()
                .filter(|(_, bot)| matches!(bot.state, BotState::WaitingWarmup))
                .map(|(id, bot)| (id.clone(), bot.config.symbol.clone()))
                .collect()
        };

        for (bot_id, symbol) in bots_waiting {
            let warmup_status = self.warmup_checker.check_symbol(&symbol).await;

            if warmup_status.ready {
                info!(bot_id = bot_id, "Warmup data ready, starting bot");
                if let Err(e) = self.start_bot(&bot_id).await {
                    error!(bot_id = bot_id, error = %e, "Failed to start bot after warmup ready");
                }
            }
        }
    }

    /// Spawn the bot process
    async fn spawn_bot_process(&self, bot: &mut ManagedBot) -> Result<()> {
        let bot_id = &bot.config.id;
        let strategy = &bot.config.strategy;

        // Determine binary path
        let binary_path = self.bin_dir.join(strategy);

        if !binary_path.exists() {
            anyhow::bail!(
                "Strategy binary not found: {}",
                binary_path.display()
            );
        }

        // Build command
        let mut cmd = Command::new(&binary_path);
        cmd.arg("--config")
            .arg(&bot.config.config_file)
            .arg("--symbol")
            .arg(&bot.config.symbol);

        // Pass connector name if specified, otherwise use default connector
        let config = self.config.read().await;
        let connector_name = bot.config.connector.clone()
            .or_else(|| config.default_connector_name().map(|s| s.to_string()));
        drop(config);

        if let Some(connector) = connector_name {
            cmd.arg("--connector").arg(&connector);
        }

        // Set up logging
        let config = self.config.read().await;
        let log_dir = config.cluster.log_dir.join("bots");
        std::fs::create_dir_all(&log_dir)?;

        let log_file = std::fs::File::create(log_dir.join(format!("{}.log", bot_id)))?;
        cmd.stdout(Stdio::from(log_file.try_clone()?));
        cmd.stderr(Stdio::from(log_file));

        // Spawn process
        let child = cmd.spawn().with_context(|| {
            format!(
                "Failed to spawn bot process: {}",
                binary_path.display()
            )
        })?;

        let pid = child.id().unwrap_or(0);
        info!(bot_id = bot_id, pid = pid, "Bot process started");

        bot.process = Some(child);
        bot.pid = Some(pid);
        bot.started_at = Some(Instant::now());

        let old_state = std::mem::replace(&mut bot.state, BotState::Running);

        self.emit_event(ManagerEvent::BotStarted {
            bot_id: bot_id.clone(),
            pid,
        });
        self.emit_event(ManagerEvent::BotStateChanged {
            bot_id: bot_id.clone(),
            old_state,
            new_state: BotState::Running,
        });

        Ok(())
    }

    fn emit_event(&self, event: ManagerEvent) {
        // Ignore send errors (no subscribers)
        let _ = self.event_tx.send(event);
    }
}
