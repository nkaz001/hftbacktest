import { useNavigate } from 'react-router-dom';
import { Menu, LogOut, User } from 'lucide-react';
import { Button } from '@/components/ui/Button';
import { useStore } from '@/stores/useStore';
import { api } from '@/api/client';

export function Header() {
  const navigate = useNavigate();
  const { user, setUser, sidebarOpen, setSidebarOpen, status } = useStore();

  const handleLogout = async () => {
    try {
      await api.logout();
    } finally {
      setUser(null);
      navigate('/login');
    }
  };

  return (
    <header className="h-16 bg-[hsl(var(--card))] border-b border-[hsl(var(--border))] flex items-center justify-between px-6">
      <div className="flex items-center">
        <Button
          variant="ghost"
          size="icon"
          onClick={() => setSidebarOpen(!sidebarOpen)}
          className="mr-4"
        >
          <Menu className="h-5 w-5" />
        </Button>

        {status && (
          <div className="text-sm text-[hsl(var(--muted-foreground))]">
            <span className="font-medium text-[hsl(var(--foreground))]">
              {status.cluster_name}
            </span>
            <span className="mx-2">•</span>
            <span>Uptime: {formatUptime(status.uptime_secs)}</span>
          </div>
        )}
      </div>

      <div className="flex items-center space-x-4">
        {user && (
          <>
            <div className="flex items-center text-sm">
              <User className="h-4 w-4 mr-2 text-[hsl(var(--muted-foreground))]" />
              <span>{user.username}</span>
              <span className="ml-2 px-2 py-0.5 rounded bg-[hsl(var(--secondary))] text-xs">
                {user.role}
              </span>
            </div>
            <Button variant="ghost" size="sm" onClick={handleLogout}>
              <LogOut className="h-4 w-4 mr-2" />
              Logout
            </Button>
          </>
        )}
      </div>
    </header>
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
