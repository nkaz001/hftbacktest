import { useState } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import {
  Settings,
  FileCode,
  Loader2,
  ChevronRight,
  Edit3,
  Save,
  X,
  AlertCircle,
  CheckCircle,
} from 'lucide-react';
import { Card, CardHeader, CardTitle, CardContent } from '@/components/ui/Card';
import { Button } from '@/components/ui/Button';
import { api } from '@/api/client';
import type { ClusterConfig, StrategyConfig } from '@/types';

// Helper to convert JSON back to TOML-like format for display
function jsonToToml(obj: unknown, indent = 0): string {
  if (obj === null || obj === undefined) return '';

  const spaces = '  '.repeat(indent);
  let result = '';

  if (typeof obj !== 'object') {
    if (typeof obj === 'string') return `"${obj}"`;
    return String(obj);
  }

  if (Array.isArray(obj)) {
    if (obj.length === 0) return '[]';
    if (typeof obj[0] !== 'object') {
      return `[${obj.map(v => typeof v === 'string' ? `"${v}"` : v).join(', ')}]`;
    }
    // Array of tables
    return obj.map(item => jsonToToml(item, indent)).join('\n');
  }

  const entries = Object.entries(obj as Record<string, unknown>);
  for (const [key, value] of entries) {
    if (value === null || value === undefined) continue;

    if (typeof value === 'object' && !Array.isArray(value)) {
      result += `${spaces}[${key}]\n`;
      result += jsonToToml(value, indent);
      result += '\n';
    } else {
      result += `${spaces}${key} = ${jsonToToml(value, indent)}\n`;
    }
  }

  return result;
}

interface ConfigEditorProps {
  title: string;
  content: string;
  onSave: (content: string) => Promise<void>;
  onCancel: () => void;
  isLoading: boolean;
  error: string | null;
}

function ConfigEditor({ title, content, onSave, onCancel, isLoading, error }: ConfigEditorProps) {
  const [editedContent, setEditedContent] = useState(content);

  const handleSave = async () => {
    await onSave(editedContent);
  };

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <h3 className="font-medium">{title}</h3>
        <div className="flex items-center space-x-2">
          <Button
            size="sm"
            variant="ghost"
            onClick={onCancel}
            disabled={isLoading}
          >
            <X className="h-4 w-4 mr-1" />
            Cancel
          </Button>
          <Button
            size="sm"
            onClick={handleSave}
            disabled={isLoading}
          >
            {isLoading ? (
              <Loader2 className="h-4 w-4 mr-1 animate-spin" />
            ) : (
              <Save className="h-4 w-4 mr-1" />
            )}
            Save
          </Button>
        </div>
      </div>

      {error && (
        <div className="flex items-center p-3 rounded-lg bg-destructive/10 text-destructive text-sm">
          <AlertCircle className="h-4 w-4 mr-2 flex-shrink-0" />
          <span className="font-mono text-xs">{error}</span>
        </div>
      )}

      <textarea
        value={editedContent}
        onChange={(e) => setEditedContent(e.target.value)}
        className="w-full h-[400px] p-4 rounded-lg bg-[hsl(var(--muted))] font-mono text-sm border border-[hsl(var(--border))] focus:outline-none focus:ring-2 focus:ring-[hsl(var(--primary))] resize-none"
        spellCheck={false}
      />

      <p className="text-xs text-[hsl(var(--muted-foreground))]">
        Edit the TOML configuration above. Changes will be validated before saving.
      </p>
    </div>
  );
}

export function Config() {
  const queryClient = useQueryClient();
  const [selectedStrategy, setSelectedStrategy] = useState<string | null>(null);
  const [editingCluster, setEditingCluster] = useState(false);
  const [editingStrategy, setEditingStrategy] = useState(false);
  const [clusterRawContent, setClusterRawContent] = useState<string | null>(null);
  const [strategyRawContent, setStrategyRawContent] = useState<string | null>(null);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [saveSuccess, setSaveSuccess] = useState(false);

  const { data: clusterConfig, isLoading: clusterLoading } = useQuery<ClusterConfig>({
    queryKey: ['cluster-config'],
    queryFn: () => api.getClusterConfig(),
  });

  const { data: strategies, isLoading: strategiesLoading } = useQuery<StrategyConfig[]>({
    queryKey: ['strategy-configs'],
    queryFn: () => api.getStrategyConfigs(),
  });

  const { data: strategyConfig, isLoading: strategyLoading } = useQuery({
    queryKey: ['strategy-config', selectedStrategy],
    queryFn: () => api.getStrategyConfig(selectedStrategy!),
    enabled: !!selectedStrategy,
  });

  const updateClusterMutation = useMutation({
    mutationFn: (content: string) => api.updateClusterConfig(content),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['cluster-config'] });
      setEditingCluster(false);
      setClusterRawContent(null);
      setSaveError(null);
      setSaveSuccess(true);
      setTimeout(() => setSaveSuccess(false), 3000);
    },
    onError: (error: Error) => {
      setSaveError(error.message);
    },
  });

  const updateStrategyMutation = useMutation({
    mutationFn: ({ botId, content }: { botId: string; content: string }) =>
      api.updateStrategyConfig(botId, content),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['strategy-config', selectedStrategy] });
      setEditingStrategy(false);
      setStrategyRawContent(null);
      setSaveError(null);
      setSaveSuccess(true);
      setTimeout(() => setSaveSuccess(false), 3000);
    },
    onError: (error: Error) => {
      setSaveError(error.message);
    },
  });

  const handleEditCluster = async () => {
    setSaveError(null);
    try {
      // Fetch raw TOML content from server
      const rawContent = await api.getClusterConfigRaw();
      setClusterRawContent(rawContent);
    } catch {
      // Fallback to JSON conversion if raw endpoint fails
      if (clusterConfig) {
        setClusterRawContent(jsonToToml(clusterConfig));
      }
    }
    setEditingCluster(true);
  };

  const handleEditStrategy = async () => {
    if (!selectedStrategy) return;
    setSaveError(null);
    try {
      // Fetch raw TOML content from server
      const rawContent = await api.getStrategyConfigRaw(selectedStrategy);
      setStrategyRawContent(rawContent);
    } catch {
      // Fallback to JSON conversion if raw endpoint fails
      if (strategyConfig) {
        setStrategyRawContent(jsonToToml(strategyConfig));
      }
    }
    setEditingStrategy(true);
  };

  if (clusterLoading || strategiesLoading) {
    return (
      <div className="flex items-center justify-center h-64">
        <Loader2 className="h-8 w-8 animate-spin text-[hsl(var(--muted-foreground))]" />
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <h1 className="text-3xl font-bold">Configuration</h1>
        {saveSuccess && (
          <div className="flex items-center text-green-500 text-sm">
            <CheckCircle className="h-4 w-4 mr-1" />
            Configuration saved successfully
          </div>
        )}
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        {/* Cluster Config */}
        <Card>
          <CardHeader>
            <div className="flex items-center justify-between">
              <CardTitle className="flex items-center">
                <Settings className="h-5 w-5 mr-2" />
                Cluster Configuration
              </CardTitle>
              {!editingCluster && (
                <Button size="sm" variant="ghost" onClick={handleEditCluster}>
                  <Edit3 className="h-4 w-4 mr-1" />
                  Edit
                </Button>
              )}
            </div>
          </CardHeader>
          <CardContent>
            {editingCluster && clusterRawContent !== null ? (
              <ConfigEditor
                title="Edit Cluster Configuration"
                content={clusterRawContent}
                onSave={async (content) => {
                  await updateClusterMutation.mutateAsync(content);
                }}
                onCancel={() => {
                  setEditingCluster(false);
                  setClusterRawContent(null);
                  setSaveError(null);
                }}
                isLoading={updateClusterMutation.isPending}
                error={saveError}
              />
            ) : clusterConfig ? (
              <dl className="space-y-4">
                <div>
                  <dt className="text-sm text-[hsl(var(--muted-foreground))]">Cluster Name</dt>
                  <dd className="font-medium">{clusterConfig.cluster.name}</dd>
                </div>
                <div>
                  <dt className="text-sm text-[hsl(var(--muted-foreground))]">Data Directory</dt>
                  <dd className="font-mono text-sm">{clusterConfig.cluster.data_dir}</dd>
                </div>
                <div>
                  <dt className="text-sm text-[hsl(var(--muted-foreground))]">Log Directory</dt>
                  <dd className="font-mono text-sm">{clusterConfig.cluster.log_dir}</dd>
                </div>
                <div>
                  <dt className="text-sm text-[hsl(var(--muted-foreground))]">Connectors</dt>
                  <dd className="font-medium">
                    {clusterConfig.connectors && clusterConfig.connectors.length > 0 ? (
                      <ul className="space-y-1">
                        {clusterConfig.connectors.map((conn) => (
                          <li key={conn.name}>
                            {conn.name} ({conn.type})
                          </li>
                        ))}
                      </ul>
                    ) : clusterConfig.connector ? (
                      <span>{clusterConfig.connector.name} ({clusterConfig.connector.type})</span>
                    ) : (
                      <span className="text-[hsl(var(--muted-foreground))]">None configured</span>
                    )}
                  </dd>
                </div>
                <div>
                  <dt className="text-sm text-[hsl(var(--muted-foreground))]">Collector</dt>
                  <dd className="font-medium">
                    {clusterConfig.collector.enabled ? 'Enabled' : 'Disabled'}
                    {clusterConfig.collector.enabled && (
                      <span className="text-[hsl(var(--muted-foreground))] ml-2">
                        ({clusterConfig.collector.symbols.join(', ')})
                      </span>
                    )}
                  </dd>
                </div>
                <div>
                  <dt className="text-sm text-[hsl(var(--muted-foreground))]">Bot Defaults</dt>
                  <dd className="text-sm">
                    Tick: {clusterConfig.bot_defaults.tick_size}, Lot: {clusterConfig.bot_defaults.lot_size}
                  </dd>
                </div>
              </dl>
            ) : null}
          </CardContent>
        </Card>

        {/* Strategy Configs */}
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center">
              <FileCode className="h-5 w-5 mr-2" />
              Strategy Configurations
            </CardTitle>
          </CardHeader>
          <CardContent>
            {strategies && strategies.length > 0 ? (
              <div className="space-y-2">
                {strategies.map((strategy) => (
                  <button
                    key={strategy.bot_id}
                    onClick={() => {
                      setSelectedStrategy(strategy.bot_id);
                      setEditingStrategy(false);
                      setStrategyRawContent(null);
                      setSaveError(null);
                    }}
                    className={`w-full flex items-center justify-between p-3 rounded-lg border transition-colors ${
                      selectedStrategy === strategy.bot_id
                        ? 'border-[hsl(var(--primary))] bg-[hsl(var(--accent))]'
                        : 'border-[hsl(var(--border))] hover:bg-[hsl(var(--accent))]/50'
                    }`}
                  >
                    <div className="text-left">
                      <div className="font-medium">{strategy.bot_id}</div>
                      <div className="text-xs text-[hsl(var(--muted-foreground))] truncate max-w-[200px]">
                        {strategy.config_path}
                      </div>
                    </div>
                    <ChevronRight className="h-4 w-4 text-[hsl(var(--muted-foreground))]" />
                  </button>
                ))}
              </div>
            ) : (
              <div className="text-center py-8 text-[hsl(var(--muted-foreground))]">
                No strategy configurations found
              </div>
            )}
          </CardContent>
        </Card>
      </div>

      {/* Selected Strategy Config */}
      {selectedStrategy && (
        <Card>
          <CardHeader>
            <div className="flex items-center justify-between">
              <CardTitle>Strategy: {selectedStrategy}</CardTitle>
              {!editingStrategy && strategyConfig && (
                <Button size="sm" variant="ghost" onClick={handleEditStrategy}>
                  <Edit3 className="h-4 w-4 mr-1" />
                  Edit
                </Button>
              )}
            </div>
          </CardHeader>
          <CardContent>
            {strategyLoading ? (
              <div className="flex items-center justify-center py-8">
                <Loader2 className="h-6 w-6 animate-spin" />
              </div>
            ) : editingStrategy && strategyRawContent !== null ? (
              <ConfigEditor
                title={`Edit Strategy: ${selectedStrategy}`}
                content={strategyRawContent}
                onSave={async (content) => {
                  await updateStrategyMutation.mutateAsync({
                    botId: selectedStrategy,
                    content,
                  });
                }}
                onCancel={() => {
                  setEditingStrategy(false);
                  setStrategyRawContent(null);
                  setSaveError(null);
                }}
                isLoading={updateStrategyMutation.isPending}
                error={saveError}
              />
            ) : strategyConfig ? (
              <pre className="p-4 rounded-lg bg-[hsl(var(--muted))] overflow-auto text-sm font-mono max-h-[400px]">
                {JSON.stringify(strategyConfig, null, 2)}
              </pre>
            ) : (
              <div className="text-center py-8 text-[hsl(var(--muted-foreground))]">
                Failed to load strategy configuration
              </div>
            )}
          </CardContent>
        </Card>
      )}
    </div>
  );
}
