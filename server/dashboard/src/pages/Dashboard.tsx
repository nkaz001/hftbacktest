import { Activity, Server, Database, Bot, CheckCircle, XCircle } from 'lucide-react';
import { Card, CardHeader, CardTitle, CardContent } from '@/components/ui/Card';
import { Badge } from '@/components/ui/Badge';
import { useStore } from '@/stores/useStore';
import { useBots } from '@/hooks/useBots';

export function Dashboard() {
  const { status } = useStore();
  const { data: bots } = useBots();

  const runningBots = bots?.filter((b) => b.state === 'running').length ?? 0;
  const totalBots = bots?.length ?? 0;

  return (
    <div className="space-y-6">
      <h1 className="text-3xl font-bold">Dashboard</h1>

      {/* Status Cards */}
      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-6">
        {/* Connector Status */}
        <Card>
          <CardHeader className="flex flex-row items-center justify-between pb-2">
            <CardTitle className="text-sm font-medium">Connector</CardTitle>
            <Server className="h-4 w-4 text-[hsl(var(--muted-foreground))]" />
          </CardHeader>
          <CardContent>
            <div className="flex items-center space-x-2">
              {status?.connector.healthy ? (
                <CheckCircle className="h-5 w-5 text-[hsl(var(--success))]" />
              ) : (
                <XCircle className="h-5 w-5 text-[hsl(var(--destructive))]" />
              )}
              <span className="text-2xl font-bold">
                {status?.connector.status ?? 'Unknown'}
              </span>
            </div>
            <p className="text-xs text-[hsl(var(--muted-foreground))] mt-1">
              Exchange connection
            </p>
          </CardContent>
        </Card>

        {/* Collector Status */}
        <Card>
          <CardHeader className="flex flex-row items-center justify-between pb-2">
            <CardTitle className="text-sm font-medium">Collector</CardTitle>
            <Database className="h-4 w-4 text-[hsl(var(--muted-foreground))]" />
          </CardHeader>
          <CardContent>
            <div className="flex items-center space-x-2">
              {status?.collector.healthy ? (
                <CheckCircle className="h-5 w-5 text-[hsl(var(--success))]" />
              ) : (
                <XCircle className="h-5 w-5 text-[hsl(var(--destructive))]" />
              )}
              <span className="text-2xl font-bold">
                {status?.collector.status ?? 'Unknown'}
              </span>
            </div>
            <p className="text-xs text-[hsl(var(--muted-foreground))] mt-1">
              Data collection
            </p>
          </CardContent>
        </Card>

        {/* Bots Status */}
        <Card>
          <CardHeader className="flex flex-row items-center justify-between pb-2">
            <CardTitle className="text-sm font-medium">Bots</CardTitle>
            <Bot className="h-4 w-4 text-[hsl(var(--muted-foreground))]" />
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">
              {runningBots} / {totalBots}
            </div>
            <p className="text-xs text-[hsl(var(--muted-foreground))] mt-1">
              Running / Total
            </p>
          </CardContent>
        </Card>

        {/* Uptime */}
        <Card>
          <CardHeader className="flex flex-row items-center justify-between pb-2">
            <CardTitle className="text-sm font-medium">Uptime</CardTitle>
            <Activity className="h-4 w-4 text-[hsl(var(--muted-foreground))]" />
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">
              {formatUptime(status?.uptime_secs ?? 0)}
            </div>
            <p className="text-xs text-[hsl(var(--muted-foreground))] mt-1">
              Supervisor uptime
            </p>
          </CardContent>
        </Card>
      </div>

      {/* Bot List */}
      <Card>
        <CardHeader>
          <CardTitle>Active Bots</CardTitle>
        </CardHeader>
        <CardContent>
          {bots && bots.length > 0 ? (
            <div className="space-y-3">
              {bots.map((bot) => (
                <div
                  key={bot.id}
                  className="flex items-center justify-between p-4 rounded-lg border border-[hsl(var(--border))] hover:bg-[hsl(var(--accent))]/50 transition-colors"
                >
                  <div className="flex items-center space-x-4">
                    <div
                      className={`w-3 h-3 rounded-full ${
                        bot.state === 'running'
                          ? 'bg-[hsl(var(--success))]'
                          : bot.state === 'crashed'
                          ? 'bg-[hsl(var(--destructive))]'
                          : 'bg-[hsl(var(--muted-foreground))]'
                      }`}
                    />
                    <div>
                      <div className="font-medium">{bot.id}</div>
                      <div className="text-sm text-[hsl(var(--muted-foreground))]">
                        {bot.symbol.toUpperCase()}
                      </div>
                    </div>
                  </div>

                  <div className="flex items-center space-x-4">
                    {bot.pid && (
                      <span className="text-sm text-[hsl(var(--muted-foreground))]">
                        PID: {bot.pid}
                      </span>
                    )}
                    {bot.uptime_secs != null && (
                      <span className="text-sm text-[hsl(var(--muted-foreground))]">
                        {formatUptime(bot.uptime_secs)}
                      </span>
                    )}
                    <Badge
                      variant={
                        bot.state === 'running'
                          ? 'success'
                          : bot.state === 'crashed'
                          ? 'destructive'
                          : 'outline'
                      }
                    >
                      {bot.state}
                    </Badge>
                  </div>
                </div>
              ))}
            </div>
          ) : (
            <div className="text-center py-8 text-[hsl(var(--muted-foreground))]">
              No bots configured
            </div>
          )}
        </CardContent>
      </Card>
    </div>
  );
}

function formatUptime(seconds: number): string {
  const days = Math.floor(seconds / 86400);
  const hours = Math.floor((seconds % 86400) / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);

  const parts = [];
  if (days > 0) parts.push(`${days}d`);
  if (hours > 0) parts.push(`${hours}h`);
  if (minutes > 0 || parts.length === 0) parts.push(`${minutes}m`);

  return parts.join(' ');
}
