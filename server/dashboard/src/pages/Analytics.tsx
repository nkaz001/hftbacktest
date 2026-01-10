import { useState } from 'react';
import { BarChart3, ExternalLink, RefreshCw, Maximize2 } from 'lucide-react';
import { Card, CardHeader, CardTitle, CardContent } from '@/components/ui/Card';
import { Button } from '@/components/ui/Button';

// Grafana URL configuration
// In production, this should be configurable via environment variables
const GRAFANA_BASE_URL = window.location.hostname === 'localhost'
  ? 'http://localhost:13001'
  : `http://${window.location.hostname}:13001`;

const dashboards = [
  {
    id: 'hft-trading-overview',
    name: 'Trading Overview',
    description: 'Orders, fills, latency metrics and recent activity'
  },
  {
    id: 'hft-position-monitor',
    name: 'Position Monitor',
    description: 'Real-time position tracking, unrealized PnL, and position history by symbol'
  },
  {
    id: 'hft-pnl-tracking',
    name: 'PnL Tracking',
    description: 'Cumulative PnL, realized profits, commissions, funding fees breakdown'
  },
  {
    id: 'hft-trading-activity',
    name: 'Trading Activity',
    description: 'Trading volume, trade count, maker/taker ratio, buy/sell distribution'
  },
];

export function Analytics() {
  const [selectedDashboard, setSelectedDashboard] = useState(dashboards[0].id);
  const [isFullscreen, setIsFullscreen] = useState(false);
  const [refreshKey, setRefreshKey] = useState(0);

  // Build Grafana URL with kiosk mode for clean embedding
  const grafanaUrl = `${GRAFANA_BASE_URL}/d/${selectedDashboard}/${selectedDashboard}?orgId=1&kiosk&refresh=30s`;

  const handleRefresh = () => {
    setRefreshKey(prev => prev + 1);
  };

  const openInGrafana = () => {
    window.open(`${GRAFANA_BASE_URL}/d/${selectedDashboard}/${selectedDashboard}`, '_blank');
  };

  const toggleFullscreen = () => {
    setIsFullscreen(!isFullscreen);
  };

  if (isFullscreen) {
    return (
      <div className="fixed inset-0 z-50 bg-[hsl(var(--background))]">
        <div className="absolute top-4 right-4 z-10 flex space-x-2">
          <Button variant="outline" size="sm" onClick={handleRefresh}>
            <RefreshCw className="h-4 w-4 mr-1" />
            Refresh
          </Button>
          <Button variant="outline" size="sm" onClick={openInGrafana}>
            <ExternalLink className="h-4 w-4 mr-1" />
            Open in Grafana
          </Button>
          <Button variant="outline" size="sm" onClick={toggleFullscreen}>
            <Maximize2 className="h-4 w-4 mr-1" />
            Exit Fullscreen
          </Button>
        </div>
        <iframe
          key={refreshKey}
          src={grafanaUrl}
          className="w-full h-full border-0"
          title="Grafana Dashboard"
        />
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div className="flex items-center space-x-3">
          <BarChart3 className="h-8 w-8 text-[hsl(var(--primary))]" />
          <h1 className="text-3xl font-bold">Analytics</h1>
        </div>
        <div className="flex items-center space-x-2">
          <Button variant="outline" size="sm" onClick={handleRefresh}>
            <RefreshCw className="h-4 w-4 mr-1" />
            Refresh
          </Button>
          <Button variant="outline" size="sm" onClick={toggleFullscreen}>
            <Maximize2 className="h-4 w-4 mr-1" />
            Fullscreen
          </Button>
          <Button variant="outline" size="sm" onClick={openInGrafana}>
            <ExternalLink className="h-4 w-4 mr-1" />
            Open in Grafana
          </Button>
        </div>
      </div>

      {/* Dashboard Selector */}
      <div className="flex flex-wrap gap-3">
        {dashboards.map((dashboard) => (
          <button
            key={dashboard.id}
            onClick={() => setSelectedDashboard(dashboard.id)}
            className={`px-4 py-2 rounded-lg border transition-colors ${
              selectedDashboard === dashboard.id
                ? 'bg-[hsl(var(--primary))] text-[hsl(var(--primary-foreground))] border-[hsl(var(--primary))]'
                : 'bg-[hsl(var(--card))] border-[hsl(var(--border))] hover:bg-[hsl(var(--accent))] text-[hsl(var(--foreground))]'
            }`}
          >
            <span className="font-medium">{dashboard.name}</span>
          </button>
        ))}
      </div>

      {/* Dashboard Description */}
      <p className="text-[hsl(var(--muted-foreground))] text-sm">
        {dashboards.find(d => d.id === selectedDashboard)?.description}
      </p>

      {/* Grafana Iframe */}
      <Card className="overflow-hidden">
        <CardHeader className="pb-2">
          <CardTitle className="flex items-center space-x-2">
            <BarChart3 className="h-5 w-5" />
            <span>{dashboards.find(d => d.id === selectedDashboard)?.name}</span>
          </CardTitle>
        </CardHeader>
        <CardContent className="p-0">
          <div className="relative" style={{ height: 'calc(100vh - 320px)', minHeight: '600px' }}>
            <iframe
              key={refreshKey}
              src={grafanaUrl}
              className="absolute inset-0 w-full h-full border-0"
              title="Grafana Dashboard"
            />
          </div>
        </CardContent>
      </Card>

      {/* Info Card */}
      <Card>
        <CardContent className="py-4">
          <p className="text-sm text-[hsl(var(--muted-foreground))]">
            Data is sourced from ClickHouse and refreshes automatically every 30 seconds.
            Use the template variables in Grafana to filter by symbol or strategy.
            For detailed analysis, click "Open in Grafana" to access the full Grafana interface.
          </p>
        </CardContent>
      </Card>
    </div>
  );
}
