// Bot types
export type BotState = 'running' | 'stopped' | 'starting' | 'crashed';

export interface Bot {
  id: string;
  symbol: string;
  state: BotState;
  pid: number | null;
  uptime_secs: number | null;
  restart_count: number;
}

export interface BotConfig {
  id: string;
  enabled: boolean;
  strategy: string;
  symbol: string;
  config_file: string;
  auto_restart: boolean;
  max_restarts: number;
  restart_backoff_secs: number;
}

// Cluster types
export interface ClusterStatus {
  cluster_name: string;
  uptime_secs: number;
  connector: ComponentStatus;
  collector: ComponentStatus;
  bots: Bot[];
}

export interface ComponentStatus {
  status: string;
  healthy: boolean;
}

// Config types
export interface ClusterConfig {
  cluster: {
    name: string;
    data_dir: string;
    log_dir: string;
    socket_path: string;
    config_reload_interval_secs: number;
  };
  connector: {
    name: string;
    type: string;
    config_file: string;
  };
  collector: {
    enabled: boolean;
    mode: string;
    output_dir: string;
    symbols: string[];
    retention_days: number;
  };
  warmup: {
    data_dir: string;
    max_age_seconds: number;
    min_duration_seconds: number;
  };
  bot_defaults: {
    tick_size: number;
    lot_size: number;
  };
  bots: BotConfig[];
}

export interface StrategyConfig {
  bot_id: string;
  config_path: string;
  exists: boolean;
}

// Auth types
export interface User {
  id: number;
  username: string;
  role: string;
}

export interface LoginResponse {
  token: string;
  expires_at: string;
  user: User;
}

// WebSocket event types
export interface WsStatusUpdate {
  type: 'status_update';
  cluster_name: string;
  uptime_secs: number;
  connector: ComponentStatus;
  collector: ComponentStatus;
  bots: Bot[];
}

// API response types
export interface ApiResponse<T> {
  data?: T;
  error?: string;
  message?: string;
}

export interface BotActionResponse {
  success: boolean;
  bot_id: string;
  action: string;
  message: string;
}

// Audit log types
export interface AuditLog {
  id: number;
  user_id: number | null;
  action: string;
  resource_type: string;
  resource_id: string | null;
  details: string | null;
  ip_address: string | null;
  created_at: string;
}
