import { Database, HardDrive } from 'lucide-react';
import { Card, CardHeader, CardTitle, CardContent } from '@/components/ui/Card';

export function Data() {
  return (
    <div className="space-y-6">
      <h1 className="text-3xl font-bold">Data Management</h1>

      <Card>
        <CardHeader>
          <CardTitle className="flex items-center">
            <Database className="h-5 w-5 mr-2" />
            Data Files
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="flex flex-col items-center justify-center py-12 text-[hsl(var(--muted-foreground))]">
            <HardDrive className="h-12 w-12 mb-4" />
            <p>Data file management coming soon</p>
            <p className="text-sm mt-2">View, download, and clean up market data files</p>
          </div>
        </CardContent>
      </Card>
    </div>
  );
}
