import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { api } from '@/api/client';
import type { Bot, BotActionResponse } from '@/types';

export function useBots() {
  return useQuery<Bot[]>({
    queryKey: ['bots'],
    queryFn: () => api.getBots(),
    refetchInterval: 5000, // Refresh every 5 seconds as fallback
  });
}

export function useBot(id: string) {
  return useQuery<Bot>({
    queryKey: ['bot', id],
    queryFn: () => api.getBot(id),
    enabled: !!id,
  });
}

export function useStartBot() {
  const queryClient = useQueryClient();

  return useMutation<BotActionResponse, Error, string>({
    mutationFn: (id: string) => api.startBot(id),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['bots'] });
    },
  });
}

export function useStopBot() {
  const queryClient = useQueryClient();

  return useMutation<BotActionResponse, Error, string>({
    mutationFn: (id: string) => api.stopBot(id),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['bots'] });
    },
  });
}

export function useRestartBot() {
  const queryClient = useQueryClient();

  return useMutation<BotActionResponse, Error, string>({
    mutationFn: (id: string) => api.restartBot(id),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['bots'] });
    },
  });
}
