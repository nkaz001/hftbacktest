import { Play, Square, RefreshCw, Loader2 } from 'lucide-react';
import { Card, CardHeader, CardTitle, CardContent, CardFooter } from '@/components/ui/Card';
import { Button } from '@/components/ui/Button';
import { Badge } from '@/components/ui/Badge';
import { useBots, useStartBot, useStopBot, useRestartBot } from '@/hooks/useBots';

export function Bots() {
  const { data: bots, isLoading } = useBots();
  const startBot = useStartBot();
  const stopBot = useStopBot();
  const restartBot = useRestartBot();

  if (isLoading) {
    return (
      <div className="flex items-center justify-center h-64">
        <Loader2 className="h-8 w-8 animate-spin text-[hsl(var(--muted-foreground))]" />
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <h1 className="text-3xl font-bold">Bots</h1>
      </div>

      {bots && bots.length > 0 ? (
        <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
          {bots.map((bot) => (
            <Card key={bot.id}>
              <CardHeader>
                <div className="flex items-center justify-between">
                  <CardTitle className="flex items-center">
                    <span
                      className={`w-3 h-3 rounded-full mr-3 ${
                        bot.state === 'running'
                          ? 'bg-[hsl(var(--success))]'
                          : bot.state === 'crashed'
                          ? 'bg-[hsl(var(--destructive))]'
                          : 'bg-[hsl(var(--muted-foreground))]'
                      }`}
                    />
                    {bot.id}
                  </CardTitle>
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
              </CardHeader>
              <CardContent>
                <dl className="grid grid-cols-2 gap-4 text-sm">
                  <div>
                    <dt className="text-[hsl(var(--muted-foreground))]">Symbol</dt>
                    <dd className="font-medium">{bot.symbol.toUpperCase() || '-'}</dd>
                  </div>
                  <div>
                    <dt className="text-[hsl(var(--muted-foreground))]">PID</dt>
                    <dd className="font-medium font-mono">{bot.pid ?? '-'}</dd>
                  </div>
                  <div>
                    <dt className="text-[hsl(var(--muted-foreground))]">Uptime</dt>
                    <dd className="font-medium">
                      {bot.uptime_secs != null ? formatUptime(bot.uptime_secs) : '-'}
                    </dd>
                  </div>
                  <div>
                    <dt className="text-[hsl(var(--muted-foreground))]">Restarts</dt>
                    <dd className="font-medium">{bot.restart_count}</dd>
                  </div>
                </dl>
              </CardContent>
              <CardFooter className="gap-2">
                {bot.state === 'running' ? (
                  <>
                    <Button
                      variant="destructive"
                      size="sm"
                      onClick={() => stopBot.mutate(bot.id)}
                      disabled={stopBot.isPending}
                    >
                      {stopBot.isPending ? (
                        <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                      ) : (
                        <Square className="h-4 w-4 mr-2" />
                      )}
                      Stop
                    </Button>
                    <Button
                      variant="outline"
                      size="sm"
                      onClick={() => restartBot.mutate(bot.id)}
                      disabled={restartBot.isPending}
                    >
                      {restartBot.isPending ? (
                        <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                      ) : (
                        <RefreshCw className="h-4 w-4 mr-2" />
                      )}
                      Restart
                    </Button>
                  </>
                ) : (
                  <Button
                    variant="success"
                    size="sm"
                    onClick={() => startBot.mutate(bot.id)}
                    disabled={startBot.isPending}
                  >
                    {startBot.isPending ? (
                      <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                    ) : (
                      <Play className="h-4 w-4 mr-2" />
                    )}
                    Start
                  </Button>
                )}
              </CardFooter>
            </Card>
          ))}
        </div>
      ) : (
        <Card>
          <CardContent className="py-12">
            <div className="text-center text-[hsl(var(--muted-foreground))]">
              No bots configured. Add bots to your cluster configuration.
            </div>
          </CardContent>
        </Card>
      )}
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
