import { useQuery } from '@tanstack/react-query';
import { api } from '@/api/client';
import type { ClusterStatus } from '@/types';

export function useStatus() {
  return useQuery<ClusterStatus>({
    queryKey: ['status'],
    queryFn: () => api.getStatus(),
    refetchInterval: 5000,
  });
}
