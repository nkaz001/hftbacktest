import { useQuery } from '@tanstack/react-query';
import { FileText, Loader2 } from 'lucide-react';
import { Card, CardHeader, CardTitle, CardContent } from '@/components/ui/Card';
import { api } from '@/api/client';
import type { AuditLog } from '@/types';

export function Logs() {
  const { data: logs, isLoading } = useQuery<AuditLog[]>({
    queryKey: ['audit-logs'],
    queryFn: () => api.getAuditLogs(50, 0),
    refetchInterval: 10000,
  });

  return (
    <div className="space-y-6">
      <h1 className="text-3xl font-bold">Audit Logs</h1>

      <Card>
        <CardHeader>
          <CardTitle className="flex items-center">
            <FileText className="h-5 w-5 mr-2" />
            Recent Activity
          </CardTitle>
        </CardHeader>
        <CardContent>
          {isLoading ? (
            <div className="flex items-center justify-center py-12">
              <Loader2 className="h-8 w-8 animate-spin text-[hsl(var(--muted-foreground))]" />
            </div>
          ) : logs && logs.length > 0 ? (
            <div className="overflow-x-auto">
              <table className="w-full text-sm">
                <thead>
                  <tr className="border-b border-[hsl(var(--border))]">
                    <th className="text-left py-3 px-4 font-medium">Time</th>
                    <th className="text-left py-3 px-4 font-medium">Action</th>
                    <th className="text-left py-3 px-4 font-medium">Resource</th>
                    <th className="text-left py-3 px-4 font-medium">Details</th>
                  </tr>
                </thead>
                <tbody>
                  {logs.map((log) => (
                    <tr
                      key={log.id}
                      className="border-b border-[hsl(var(--border))] hover:bg-[hsl(var(--accent))]/50"
                    >
                      <td className="py-3 px-4 font-mono text-xs">
                        {new Date(log.created_at).toLocaleString()}
                      </td>
                      <td className="py-3 px-4">
                        <span
                          className={`px-2 py-1 rounded text-xs font-medium ${
                            log.action.includes('start')
                              ? 'bg-[hsl(var(--success))]/20 text-[hsl(var(--success))]'
                              : log.action.includes('stop')
                              ? 'bg-[hsl(var(--destructive))]/20 text-[hsl(var(--destructive))]'
                              : 'bg-[hsl(var(--secondary))]'
                          }`}
                        >
                          {log.action}
                        </span>
                      </td>
                      <td className="py-3 px-4">
                        <span className="text-[hsl(var(--muted-foreground))]">
                          {log.resource_type}
                        </span>
                        {log.resource_id && (
                          <span className="ml-2 font-medium">{log.resource_id}</span>
                        )}
                      </td>
                      <td className="py-3 px-4 text-[hsl(var(--muted-foreground))]">
                        {log.details || '-'}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          ) : (
            <div className="text-center py-12 text-[hsl(var(--muted-foreground))]">
              No audit logs found
            </div>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
