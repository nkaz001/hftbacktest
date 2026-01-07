import type {
  LoginResponse,
  Bot,
  ClusterConfig,
  StrategyConfig,
  BotActionResponse,
  AuditLog,
  ClusterStatus,
  InfraStatus,
  ClusterHealth,
  InfraActionResponse
} from '@/types';

const API_BASE = '/api';

class ApiClient {
  private token: string | null = null;

  constructor() {
    // Try to restore token from localStorage
    this.token = localStorage.getItem('hft_token');
  }

  setToken(token: string | null) {
    this.token = token;
    if (token) {
      localStorage.setItem('hft_token', token);
    } else {
      localStorage.removeItem('hft_token');
    }
  }

  getToken(): string | null {
    return this.token;
  }

  private async request<T>(
    endpoint: string,
    options: RequestInit = {}
  ): Promise<T> {
    const headers: Record<string, string> = {
      'Content-Type': 'application/json',
      ...options.headers as Record<string, string>,
    };

    if (this.token) {
      headers['Authorization'] = `Bearer ${this.token}`;
    }

    const response = await fetch(`${API_BASE}${endpoint}`, {
      ...options,
      headers,
    });

    if (response.status === 401) {
      this.setToken(null);
      window.location.href = '/login';
      throw new Error('Unauthorized');
    }

    if (!response.ok) {
      const error = await response.json().catch(() => ({ message: 'Request failed' }));
      throw new Error(error.message || error.error || 'Request failed');
    }

    // Handle empty responses
    const text = await response.text();
    if (!text) {
      return {} as T;
    }

    return JSON.parse(text);
  }

  // Auth endpoints
  async login(username: string, password: string): Promise<LoginResponse> {
    const response = await this.request<LoginResponse>('/auth/login', {
      method: 'POST',
      body: JSON.stringify({ username, password }),
    });
    this.setToken(response.token);
    return response;
  }

  async logout(): Promise<void> {
    try {
      await this.request('/auth/logout', { method: 'POST' });
    } finally {
      this.setToken(null);
    }
  }

  async getMe(): Promise<{ user: { id: number; username: string; role: string } }> {
    return this.request('/auth/me');
  }

  // Bot endpoints
  async getBots(): Promise<Bot[]> {
    return this.request('/bots');
  }

  async getBot(id: string): Promise<Bot> {
    return this.request(`/bots/${id}`);
  }

  async startBot(id: string): Promise<BotActionResponse> {
    return this.request(`/bots/${id}/start`, { method: 'POST' });
  }

  async stopBot(id: string): Promise<BotActionResponse> {
    return this.request(`/bots/${id}/stop`, { method: 'POST' });
  }

  async restartBot(id: string): Promise<BotActionResponse> {
    return this.request(`/bots/${id}/restart`, { method: 'POST' });
  }

  // Status endpoint
  async getStatus(): Promise<ClusterStatus> {
    return this.request('/status');
  }

  // Config endpoints
  async getClusterConfig(): Promise<ClusterConfig> {
    return this.request('/config/cluster');
  }

  async getClusterConfigRaw(): Promise<string> {
    const response = await fetch(`${API_BASE}/config/cluster/raw`, {
      headers: this.token ? { Authorization: `Bearer ${this.token}` } : {},
    });
    if (!response.ok) {
      const error = await response.json().catch(() => ({ message: 'Request failed' }));
      throw new Error(error.message || 'Failed to fetch config');
    }
    return response.text();
  }

  async updateClusterConfig(content: string): Promise<void> {
    return this.request('/config/cluster', {
      method: 'PUT',
      body: JSON.stringify({ content }),
    });
  }

  async getStrategyConfigs(): Promise<StrategyConfig[]> {
    return this.request('/config/strategies');
  }

  async getStrategyConfig(botId: string): Promise<Record<string, unknown>> {
    return this.request(`/config/strategies/${botId}`);
  }

  async getStrategyConfigRaw(botId: string): Promise<string> {
    const response = await fetch(`${API_BASE}/config/strategies/${botId}/raw`, {
      headers: this.token ? { Authorization: `Bearer ${this.token}` } : {},
    });
    if (!response.ok) {
      const error = await response.json().catch(() => ({ message: 'Request failed' }));
      throw new Error(error.message || 'Failed to fetch config');
    }
    return response.text();
  }

  async updateStrategyConfig(botId: string, content: string): Promise<void> {
    return this.request(`/config/strategies/${botId}`, {
      method: 'PUT',
      body: JSON.stringify({ content }),
    });
  }

  // Audit log endpoint
  async getAuditLogs(limit = 50, offset = 0): Promise<AuditLog[]> {
    return this.request(`/logs/audit?limit=${limit}&offset=${offset}`);
  }

  // Infrastructure endpoints
  async getInfraList(): Promise<InfraStatus[]> {
    return this.request('/infra');
  }

  async getInfraStatus(component: string): Promise<InfraStatus> {
    return this.request(`/infra/${component}`);
  }

  async startInfra(component: string): Promise<InfraActionResponse> {
    return this.request(`/infra/${component}/start`, { method: 'POST' });
  }

  async stopInfra(component: string): Promise<InfraActionResponse> {
    return this.request(`/infra/${component}/stop`, { method: 'POST' });
  }

  async restartInfra(component: string): Promise<InfraActionResponse> {
    return this.request(`/infra/${component}/restart`, { method: 'POST' });
  }

  // Cluster management endpoints
  async startAll(): Promise<{ success: boolean; message: string }> {
    return this.request('/cluster/start-all', { method: 'POST' });
  }

  async stopAll(): Promise<{ success: boolean; message: string }> {
    return this.request('/cluster/stop-all', { method: 'POST' });
  }

  async getClusterHealth(): Promise<ClusterHealth> {
    return this.request('/cluster/health');
  }
}

export const api = new ApiClient();
