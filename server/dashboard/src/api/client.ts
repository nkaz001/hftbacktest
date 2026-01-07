import type {
  LoginResponse,
  Bot,
  ClusterConfig,
  StrategyConfig,
  BotActionResponse,
  AuditLog,
  ClusterStatus
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
}

export const api = new ApiClient();
