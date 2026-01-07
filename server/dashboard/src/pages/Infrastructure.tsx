import { useState } from 'react';
import {
  Server,
  Database,
  FileText,
  Play,
  Square,
  RotateCcw,
  AlertCircle,
  CheckCircle,
  Loader2,
  Power,
  PowerOff,
} from 'lucide-react';
import { Card, CardHeader, CardTitle, CardContent } from '@/components/ui/Card';
import { Button } from '@/components/ui/Button';
import { Badge } from '@/components/ui/Badge';
import {
  useInfraList,
  useClusterHealth,
  useStartInfra,
  useStopInfra,
  useRestartInfra,
  useStartAll,
  useStopAll,
} from '@/hooks/useInfra';
import type { InfraStatus, InfraState } from '@/types';

function formatUptime(secs: number | null): string {
  if (secs === null) return '-';
  const hours = Math.floor(secs / 3600);
  const mins = Math.floor((secs % 3600) / 60);
  if (hours > 0) {
    return `${hours}h ${mins}m`;
  }
  return `${mins}m`;
}

function getStateVariant(state: InfraState): 'default' | 'success' | 'warning' | 'destructive' | 'outline' {
  switch (state) {
    case 'running':
      return 'success';
    case 'starting':
      return 'warning';
    case 'crashed':
    case 'failed':
      return 'destructive';
    case 'disabled':
      return 'outline';
    default:
      return 'default';
  }
}

function getComponentIcon(component: string) {
  switch (component) {
    case 'connector':
      return <Server className="h-5 w-5" />;
    case 'collector':
      return <Database className="h-5 w-5" />;
    case 'order_recorder':
      return <FileText className="h-5 w-5" />;
    default:
      return <Server className="h-5 w-5" />;
  }
}

function getComponentLabel(component: string): string {
  switch (component) {
    case 'connector':
      return 'Connector';
    case 'collector':
      return 'Collector';
    case 'order_recorder':
      return 'Order Recorder';
    default:
      return component;
  }
}

function ComponentCard({ status }: { status: InfraStatus }) {
  const [actionLoading, setActionLoading] = useState<string | null>(null);
  const startMutation = useStartInfra();
  const stopMutation = useStopInfra();
  const restartMutation = useRestartInfra();

  const handleStart = async () => {
    setActionLoading('start');
    try {
      await startMutation.mutateAsync(status.component);
    } finally {
      setActionLoading(null);
    }
  };

  const handleStop = async () => {
    setActionLoading('stop');
    try {
      await stopMutation.mutateAsync(status.component);
    } finally {
      setActionLoading(null);
    }
  };

  const handleRestart = async () => {
    setActionLoading('restart');
    try {
      await restartMutation.mutateAsync(status.component);
    } finally {
      setActionLoading(null);
    }
  };

  const isRunning = status.state === 'running';
  const isDisabled = status.state === 'disabled';

  return (
    <Card>
      <CardContent className="pt-6">
        <div className="flex items-start justify-between">
          <div className="flex items-center space-x-3">
            <div className="p-2 bg-[hsl(var(--muted))] rounded-lg">
              {getComponentIcon(status.component)}
            </div>
            <div>
              <h3 className="font-semibold">{getComponentLabel(status.component)}</h3>
              <div className="flex items-center space-x-2 mt-1">
                <Badge variant={getStateVariant(status.state)}>
                  {status.state}
                </Badge>
                {status.pid && (
                  <span className="text-xs text-[hsl(var(--muted-foreground))]">
                    PID: {status.pid}
                  </span>
                )}
              </div>
            </div>
          </div>

          <div className="flex items-center space-x-2">
            {!isDisabled && (
              <>
                {!isRunning ? (
                  <Button
                    size="sm"
                    variant="ghost"
                    onClick={handleStart}
                    disabled={actionLoading !== null}
                  >
                    {actionLoading === 'start' ? (
                      <Loader2 className="h-4 w-4 animate-spin" />
                    ) : (
                      <Play className="h-4 w-4" />
                    )}
                  </Button>
                ) : (
                  <>
                    <Button
                      size="sm"
                      variant="ghost"
                      onClick={handleStop}
                      disabled={actionLoading !== null}
                    >
                      {actionLoading === 'stop' ? (
                        <Loader2 className="h-4 w-4 animate-spin" />
                      ) : (
                        <Square className="h-4 w-4" />
                      )}
                    </Button>
                    <Button
                      size="sm"
                      variant="ghost"
                      onClick={handleRestart}
                      disabled={actionLoading !== null}
                    >
                      {actionLoading === 'restart' ? (
                        <Loader2 className="h-4 w-4 animate-spin" />
                      ) : (
                        <RotateCcw className="h-4 w-4" />
                      )}
                    </Button>
                  </>
                )}
              </>
            )}
          </div>
        </div>

        <div className="mt-4 grid grid-cols-2 gap-4 text-sm">
          <div>
            <span className="text-[hsl(var(--muted-foreground))]">Uptime</span>
            <p className="font-medium">{formatUptime(status.uptime_secs)}</p>
          </div>
          {status.last_error && (
            <div className="col-span-2">
              <span className="text-[hsl(var(--muted-foreground))]">Last Error</span>
              <p className="font-medium text-destructive text-xs">{status.last_error}</p>
            </div>
          )}
        </div>
      </CardContent>
    </Card>
  );
}

export function Infrastructure() {
  const { data: infraList, isLoading: infraLoading } = useInfraList();
  const { data: health, isLoading: healthLoading } = useClusterHealth();
  const startAllMutation = useStartAll();
  const stopAllMutation = useStopAll();
  const [allActionLoading, setAllActionLoading] = useState<string | null>(null);

  const handleStartAll = async () => {
    setAllActionLoading('start');
    try {
      await startAllMutation.mutateAsync();
    } finally {
      setAllActionLoading(null);
    }
  };

  const handleStopAll = async () => {
    setAllActionLoading('stop');
    try {
      await stopAllMutation.mutateAsync();
    } finally {
      setAllActionLoading(null);
    }
  };

  if (infraLoading || healthLoading) {
    return (
      <div className="flex items-center justify-center h-64">
        <Loader2 className="h-8 w-8 animate-spin text-[hsl(var(--muted-foreground))]" />
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <h1 className="text-3xl font-bold">Infrastructure</h1>
        <div className="flex items-center space-x-2">
          <Button
            variant="outline"
            onClick={handleStartAll}
            disabled={allActionLoading !== null}
          >
            {allActionLoading === 'start' ? (
              <Loader2 className="h-4 w-4 mr-2 animate-spin" />
            ) : (
              <Power className="h-4 w-4 mr-2" />
            )}
            Start All
          </Button>
          <Button
            variant="outline"
            onClick={handleStopAll}
            disabled={allActionLoading !== null}
          >
            {allActionLoading === 'stop' ? (
              <Loader2 className="h-4 w-4 mr-2 animate-spin" />
            ) : (
              <PowerOff className="h-4 w-4 mr-2" />
            )}
            Stop All
          </Button>
        </div>
      </div>

      {/* Cluster Health Overview */}
      {health && (
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center">
              {health.all_healthy ? (
                <CheckCircle className="h-5 w-5 mr-2 text-green-500" />
              ) : (
                <AlertCircle className="h-5 w-5 mr-2 text-yellow-500" />
              )}
              Cluster Health
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
              <div className="text-center p-3 bg-[hsl(var(--muted))] rounded-lg">
                <Server className="h-6 w-6 mx-auto mb-2" />
                <div className="text-sm text-[hsl(var(--muted-foreground))]">Connector</div>
                <Badge variant={health.connector === 'running' ? 'success' : 'default'}>
                  {health.connector}
                </Badge>
              </div>
              <div className="text-center p-3 bg-[hsl(var(--muted))] rounded-lg">
                <Database className="h-6 w-6 mx-auto mb-2" />
                <div className="text-sm text-[hsl(var(--muted-foreground))]">Collector</div>
                <Badge variant={health.collector === 'running' ? 'success' : 'default'}>
                  {health.collector}
                </Badge>
              </div>
              <div className="text-center p-3 bg-[hsl(var(--muted))] rounded-lg">
                <FileText className="h-6 w-6 mx-auto mb-2" />
                <div className="text-sm text-[hsl(var(--muted-foreground))]">Order Recorder</div>
                <Badge variant={health.order_recorder === 'running' ? 'success' : 'default'}>
                  {health.order_recorder}
                </Badge>
              </div>
              <div className="text-center p-3 bg-[hsl(var(--muted))] rounded-lg">
                <Server className="h-6 w-6 mx-auto mb-2" />
                <div className="text-sm text-[hsl(var(--muted-foreground))]">Bots</div>
                <div className="text-lg font-bold">
                  {health.bots_running}/{health.bots_total}
                </div>
              </div>
            </div>
          </CardContent>
        </Card>
      )}

      {/* Component Cards */}
      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
        {infraList?.map((status) => (
          <ComponentCard key={status.component} status={status} />
        ))}
      </div>

      {(!infraList || infraList.length === 0) && (
        <Card>
          <CardContent className="py-8">
            <div className="text-center text-[hsl(var(--muted-foreground))]">
              No infrastructure components found. Make sure the supervisor is running.
            </div>
          </CardContent>
        </Card>
      )}
    </div>
  );
}
