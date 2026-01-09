// Bot types
export type BotState = 'running' | 'stopped' | 'starting' | 'crashed';

export interface Bot {
  id: string;
  symbol: string;
  connector: string | null;
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
  connector?: string;
  config_file: string;
  auto_restart: boolean;
  max_restarts: number;
  restart_backoff_secs: number;
}

// Cluster types
export interface ClusterStatus {
  cluster_name: string;
  uptime_secs: number;
  connectors: ConnectorStatus[];
  collector: ComponentStatus;
  bots: Bot[];
}

export interface ConnectorStatus {
  name: string;
  status: string;
  healthy: boolean;
}

export interface ComponentStatus {
  status: string;
  healthy: boolean;
}

// Config types
export interface ConnectorConfig {
  name: string;
  type: string;
  config_file: string;
  auto_start?: boolean;
}

export interface ClusterConfig {
  cluster: {
    name: string;
    data_dir: string;
    log_dir: string;
    socket_path: string;
    config_reload_interval_secs: number;
  };
  // Support both single connector (legacy) and multiple connectors
  connector?: ConnectorConfig;
  connectors?: ConnectorConfig[];
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
  connectors: ConnectorStatus[];
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

// Infrastructure types
// Component can be 'collector', 'order_recorder', or 'connector:name' for named connectors
export type InfraComponentType = 'connector' | 'collector' | 'order_recorder' | string;
export type InfraState = 'stopped' | 'starting' | 'running' | 'crashed' | 'failed' | 'disabled';

export interface InfraStatus {
  // Component identifier - can be 'collector', 'order_recorder', or 'connector:name'
  component: string;
  state: InfraState;
  pid: number | null;
  uptime_secs: number | null;
  last_error: string | null;
}

export interface ConnectorHealthStatus {
  name: string;
  state: string;
}

export interface ClusterHealth {
  connectors: ConnectorHealthStatus[];
  collector: string;
  order_recorder: string;
  bots_running: number;
  bots_total: number;
  all_healthy: boolean;
}

export interface InfraActionResponse {
  success: boolean;
  component: string;
  action: string;
  message: string;
}
