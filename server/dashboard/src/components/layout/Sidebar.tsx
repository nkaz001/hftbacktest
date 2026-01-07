import { NavLink } from 'react-router-dom';
import {
  LayoutDashboard,
  Bot,
  Settings,
  FileText,
  Database,
  Activity,
} from 'lucide-react';
import { useStore } from '@/stores/useStore';

const navItems = [
  { to: '/', icon: LayoutDashboard, label: 'Dashboard' },
  { to: '/bots', icon: Bot, label: 'Bots' },
  { to: '/config', icon: Settings, label: 'Config' },
  { to: '/data', icon: Database, label: 'Data' },
  { to: '/logs', icon: FileText, label: 'Logs' },
];

export function Sidebar() {
  const { sidebarOpen, wsConnected } = useStore();

  if (!sidebarOpen) {
    return null;
  }

  return (
    <aside className="w-64 bg-[hsl(var(--card))] border-r border-[hsl(var(--border))] flex flex-col">
      {/* Logo */}
      <div className="h-16 flex items-center px-6 border-b border-[hsl(var(--border))]">
        <Activity className="h-6 w-6 mr-2 text-[hsl(var(--primary))]" />
        <span className="text-xl font-bold">HFT Dashboard</span>
      </div>

      {/* Navigation */}
      <nav className="flex-1 p-4">
        <ul className="space-y-1">
          {navItems.map((item) => (
            <li key={item.to}>
              <NavLink
                to={item.to}
                className={({ isActive }) =>
                  `flex items-center px-4 py-2.5 rounded-lg transition-colors ${
                    isActive
                      ? 'bg-[hsl(var(--primary))] text-[hsl(var(--primary-foreground))]'
                      : 'hover:bg-[hsl(var(--accent))] text-[hsl(var(--muted-foreground))] hover:text-[hsl(var(--foreground))]'
                  }`
                }
              >
                <item.icon className="h-5 w-5 mr-3" />
                {item.label}
              </NavLink>
            </li>
          ))}
        </ul>
      </nav>

      {/* Connection Status */}
      <div className="p-4 border-t border-[hsl(var(--border))]">
        <div className="flex items-center text-sm">
          <span
            className={`w-2 h-2 rounded-full mr-2 ${
              wsConnected ? 'bg-[hsl(var(--success))]' : 'bg-[hsl(var(--destructive))]'
            }`}
          />
          <span className="text-[hsl(var(--muted-foreground))]">
            {wsConnected ? 'Connected' : 'Disconnected'}
          </span>
        </div>
      </div>
    </aside>
  );
}
