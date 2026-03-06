import { create } from 'zustand';
import { devtools, persist } from 'zustand/middleware';
import type { Environment, Variable } from '../types';
import { tauriApi } from '../lib/tauriApi';

interface EnvironmentState {
  environments: Environment[];
  currentEnvironment: Environment | null;
  isLoading: boolean;
  error: string | null;

  setEnvironment: (env: Environment | null) => void;
  createEnvironment: (name: string, baseUrl: string) => Environment;
  updateEnvironment: (id: string, updates: Partial<Environment>) => void;
  deleteEnvironment: (id: string) => void;
  addVariable: (envId: string, variable: Variable) => void;
  updateVariable: (envId: string, key: string, updates: Partial<Variable>) => void;
  removeVariable: (envId: string, key: string) => void;
  resolveVariables: (text: string) => string;
  resolveObjectVariables: <T extends Record<string, unknown>>(obj: T) => T;
  saveEnvironment: (env: Environment) => Promise<void>;
  loadEnvironments: () => Promise<void>;
  clearError: () => void;
}

export const useEnvironmentStore = create<EnvironmentState>()(
  devtools(
    persist(
      (set, get) => ({
        environments: [],
        currentEnvironment: null,
        isLoading: false,
        error: null,

        setEnvironment: (env: Environment | null) => {
          set({ currentEnvironment: env });
        },

        createEnvironment: (name: string, baseUrl: string) => {
          const newEnv: Environment = {
            id: crypto.randomUUID(),
            name,
            baseUrl,
            tls: { mode: 'insecure' },
            metadata: {},
            variables: [],
          };
          set((state) => ({
            environments: [...state.environments, newEnv],
          }));
          return newEnv;
        },

        updateEnvironment: (id: string, updates: Partial<Environment>) => {
          set((state) => ({
            environments: state.environments.map((env) =>
              env.id === id ? { ...env, ...updates } : env
            ),
            currentEnvironment:
              state.currentEnvironment?.id === id
                ? { ...state.currentEnvironment, ...updates }
                : state.currentEnvironment,
          }));
        },

        deleteEnvironment: (id: string) => {
          set((state) => ({
            environments: state.environments.filter((env) => env.id !== id),
            currentEnvironment:
              state.currentEnvironment?.id === id ? null : state.currentEnvironment,
          }));
        },

        addVariable: (envId: string, variable: Variable) => {
          set((state) => ({
            environments: state.environments.map((env) =>
              env.id === envId
                ? { ...env, variables: [...env.variables, variable] }
                : env
            ),
          }));
        },

        updateVariable: (envId: string, key: string, updates: Partial<Variable>) => {
          set((state) => ({
            environments: state.environments.map((env) =>
              env.id === envId
                ? {
                    ...env,
                    variables: env.variables.map((v) =>
                      v.key === key ? { ...v, ...updates } : v
                    ),
                  }
                : env
            ),
          }));
        },

        removeVariable: (envId: string, key: string) => {
          set((state) => ({
            environments: state.environments.map((env) =>
              env.id === envId
                ? { ...env, variables: env.variables.filter((v) => v.key !== key) }
                : env
            ),
          }));
        },

        resolveVariables: (text: string): string => {
          const { currentEnvironment } = get();
          if (!currentEnvironment) return text;

          return text.replace(/\{\{(\s*[\w.]+\s*)\}\}/g, (match, key) => {
            const trimmedKey = key.trim();
            const variable = currentEnvironment.variables.find(
              (v) => v.key === trimmedKey
            );
            return variable ? variable.value : match;
          });
        },

        resolveObjectVariables: <T extends Record<string, unknown>>(obj: T): T => {
          const { resolveVariables } = get();

          const resolveValue = (value: unknown): unknown => {
            if (typeof value === 'string') {
              return resolveVariables(value);
            }
            if (Array.isArray(value)) {
              return value.map(resolveValue);
            }
            if (value !== null && typeof value === 'object') {
              return resolveObject(value as Record<string, unknown>);
            }
            return value;
          };

          const resolveObject = (input: Record<string, unknown>): Record<string, unknown> => {
            const result: Record<string, unknown> = {};
            for (const [key, value] of Object.entries(input)) {
              result[key] = resolveValue(value);
            }
            return result;
          };

          return resolveObject(obj) as T;
        },

        saveEnvironment: async (env: Environment) => {
          set({ isLoading: true, error: null });
          try {
            await tauriApi.saveEnvironment(env);
            set((state) => {
              const exists = state.environments.find((e) => e.id === env.id);
              if (exists) {
                return {
                  environments: state.environments.map((e) =>
                    e.id === env.id ? env : e
                  ),
                  isLoading: false,
                };
              }
              return {
                environments: [...state.environments, env],
                isLoading: false,
              };
            });
          } catch (err) {
            set({
              error: err instanceof Error ? err.message : 'Failed to save environment',
              isLoading: false,
            });
            throw err;
          }
        },

        loadEnvironments: async () => {
          set({ isLoading: true, error: null });
          try {
            const environments = await tauriApi.getEnvironments();
            set({ environments, isLoading: false });
          } catch (err) {
            set({
              error: err instanceof Error ? err.message : 'Failed to load environments',
              isLoading: false,
            });
          }
        },

        clearError: () => {
          set({ error: null });
        },
      }),
      {
        name: 'grpcui-environment',
        partialize: (state) => ({
          environments: state.environments,
          currentEnvironment: state.currentEnvironment,
        }),
      }
    ),
    { name: 'EnvironmentStore' }
  )
);
