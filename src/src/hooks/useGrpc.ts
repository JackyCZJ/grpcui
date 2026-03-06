import {
  useQuery,
  useMutation,
  useQueryClient,
  type UseQueryResult,
  type UseMutationResult,
} from '@tanstack/react-query';
import { tauriApi } from '../lib/tauriApi';
import type { Service, GrpcResponse, History } from '../types';

// ===== Connection Hooks =====

interface ConnectVariables {
  address: string;
  tls?: {
    mode: 'insecure' | 'system' | 'custom';
    caCert?: string;
    clientCert?: string;
    clientKey?: string;
    skipVerify?: boolean;
  };
}

/**
 * Hook for connecting to a gRPC server
 */
export function useConnect(): UseMutationResult<
  { success: boolean; error?: string },
  Error,
  ConnectVariables
> {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async ({ address, tls }: ConnectVariables) => {
      return tauriApi.grpcConnect(address, tls);
    },
    onSuccess: () => {
      // Invalidate services query to trigger a refetch
      queryClient.invalidateQueries({ queryKey: ['services'] });
    },
  });
}

/**
 * Hook for fetching gRPC services
 */
export function useServices(
  enabled = false
): UseQueryResult<{ services: Service[] }, Error> {
  return useQuery({
    queryKey: ['services'],
    queryFn: tauriApi.grpcListServices,
    enabled,
    staleTime: 60000, // 1 minute
  });
}

// ===== Request Hooks =====

interface InvokeVariables {
  method: string;
  body: string;
  metadata?: Record<string, string>;
}

/**
 * Hook for invoking a unary gRPC method
 */
export function useInvoke(): UseMutationResult<
  GrpcResponse,
  Error,
  InvokeVariables
> {
  return useMutation({
    mutationFn: async (variables: InvokeVariables) => {
      return tauriApi.grpcInvoke(variables);
    },
  });
}

/**
 * Hook for starting a streaming gRPC call
 */
export function useStream(): UseMutationResult<string, Error, InvokeVariables> {
  return useMutation({
    mutationFn: async (variables: InvokeVariables) => {
      return tauriApi.grpcInvokeStream(variables);
    },
  });
}

// ===== History Hooks =====

/**
 * Hook for fetching history
 */
export function useHistories(
  limit?: number,
  enabled = true
): UseQueryResult<History[], Error> {
  return useQuery({
    queryKey: ['histories', limit],
    queryFn: () => tauriApi.getHistories(limit),
    enabled,
    staleTime: 30000, // 30 seconds
  });
}

/**
 * Hook for adding history
 */
export function useAddHistory(): UseMutationResult<void, Error, History> {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (history: History) => {
      return tauriApi.addHistory(history);
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['histories'] });
    },
  });
}

// ===== Collection Hooks =====

import type { Collection } from '../types';

/**
 * Hook for fetching collections
 */
export function useCollections(enabled = true): UseQueryResult<Collection[], Error> {
  return useQuery({
    queryKey: ['collections'],
    queryFn: tauriApi.getCollections,
    enabled,
    staleTime: 60000, // 1 minute
  });
}

/**
 * Hook for saving a collection
 */
export function useSaveCollection(): UseMutationResult<void, Error, Collection> {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (collection: Collection) => {
      return tauriApi.saveCollection(collection);
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['collections'] });
    },
  });
}

// ===== Environment Hooks =====

import type { Environment } from '../types';

/**
 * Hook for fetching environments
 */
export function useEnvironments(enabled = true): UseQueryResult<Environment[], Error> {
  return useQuery({
    queryKey: ['environments'],
    queryFn: tauriApi.getEnvironments,
    enabled,
    staleTime: 60000, // 1 minute
  });
}

/**
 * Hook for saving an environment
 */
export function useSaveEnvironment(): UseMutationResult<void, Error, Environment> {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (environment: Environment) => {
      return tauriApi.saveEnvironment(environment);
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['environments'] });
    },
  });
}
