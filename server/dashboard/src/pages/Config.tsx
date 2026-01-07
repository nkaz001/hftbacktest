import { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { Settings, FileCode, Loader2, ChevronRight } from 'lucide-react';
import { Card, CardHeader, CardTitle, CardContent } from '@/components/ui/Card';
import { api } from '@/api/client';
import type { ClusterConfig, StrategyConfig } from '@/types';

export function Config() {
  const [selectedStrategy, setSelectedStrategy] = useState<string | null>(null);

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

  if (clusterLoading || strategiesLoading) {
    return (
      <div className="flex items-center justify-center h-64">
        <Loader2 className="h-8 w-8 animate-spin text-[hsl(var(--muted-foreground))]" />
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <h1 className="text-3xl font-bold">Configuration</h1>

      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        {/* Cluster Config */}
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center">
              <Settings className="h-5 w-5 mr-2" />
              Cluster Configuration
            </CardTitle>
          </CardHeader>
          <CardContent>
            {clusterConfig && (
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
                  <dt className="text-sm text-[hsl(var(--muted-foreground))]">Connector</dt>
                  <dd className="font-medium">
                    {clusterConfig.connector.name} ({clusterConfig.connector.type})
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
            )}
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
                    onClick={() => setSelectedStrategy(strategy.bot_id)}
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
            <CardTitle>Strategy: {selectedStrategy}</CardTitle>
          </CardHeader>
          <CardContent>
            {strategyLoading ? (
              <div className="flex items-center justify-center py-8">
                <Loader2 className="h-6 w-6 animate-spin" />
              </div>
            ) : strategyConfig ? (
              <pre className="p-4 rounded-lg bg-[hsl(var(--muted))] overflow-auto text-sm font-mono">
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
