import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { api } from '@/api/client';
import type { InfraStatus, ClusterHealth, InfraActionResponse } from '@/types';

export function useInfraList() {
  return useQuery<InfraStatus[]>({
    queryKey: ['infra'],
    queryFn: () => api.getInfraList(),
    refetchInterval: 5000,
  });
}

export function useInfraStatus(component: string) {
  return useQuery<InfraStatus>({
    queryKey: ['infra', component],
    queryFn: () => api.getInfraStatus(component),
    enabled: !!component,
  });
}

export function useClusterHealth() {
  return useQuery<ClusterHealth>({
    queryKey: ['cluster-health'],
    queryFn: () => api.getClusterHealth(),
    refetchInterval: 5000,
  });
}

export function useStartInfra() {
  const queryClient = useQueryClient();

  return useMutation<InfraActionResponse, Error, string>({
    mutationFn: (component: string) => api.startInfra(component),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['infra'] });
      queryClient.invalidateQueries({ queryKey: ['cluster-health'] });
      queryClient.invalidateQueries({ queryKey: ['status'] });
    },
  });
}

export function useStopInfra() {
  const queryClient = useQueryClient();

  return useMutation<InfraActionResponse, Error, string>({
    mutationFn: (component: string) => api.stopInfra(component),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['infra'] });
      queryClient.invalidateQueries({ queryKey: ['cluster-health'] });
      queryClient.invalidateQueries({ queryKey: ['status'] });
    },
  });
}

export function useRestartInfra() {
  const queryClient = useQueryClient();

  return useMutation<InfraActionResponse, Error, string>({
    mutationFn: (component: string) => api.restartInfra(component),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['infra'] });
      queryClient.invalidateQueries({ queryKey: ['cluster-health'] });
      queryClient.invalidateQueries({ queryKey: ['status'] });
    },
  });
}

export function useStartAll() {
  const queryClient = useQueryClient();

  return useMutation<{ success: boolean; message: string }, Error>({
    mutationFn: () => api.startAll(),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['infra'] });
      queryClient.invalidateQueries({ queryKey: ['cluster-health'] });
      queryClient.invalidateQueries({ queryKey: ['status'] });
      queryClient.invalidateQueries({ queryKey: ['bots'] });
    },
  });
}

export function useStopAll() {
  const queryClient = useQueryClient();

  return useMutation<{ success: boolean; message: string }, Error>({
    mutationFn: () => api.stopAll(),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['infra'] });
      queryClient.invalidateQueries({ queryKey: ['cluster-health'] });
      queryClient.invalidateQueries({ queryKey: ['status'] });
      queryClient.invalidateQueries({ queryKey: ['bots'] });
    },
  });
}
