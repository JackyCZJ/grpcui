import { create } from 'zustand';
import { devtools, persist } from 'zustand/middleware';
import type { Service } from '../types';
import { tauriApi } from '../lib/tauriApi';

interface ConnectionState {
  isConnected: boolean;
  isConnecting: boolean;
  error: string | null;
  address: string;
  services: Service[];
  currentService: Service | null;
  currentMethod: string | null;

  connect: (address: string, tls?: TLSConfig) => Promise<void>;
  disconnect: () => void;
  loadServices: () => Promise<void>;
  selectService: (service: Service | null) => void;
  selectMethod: (method: string | null) => void;
  clearError: () => void;
}

interface TLSConfig {
  mode: 'insecure' | 'system' | 'custom';
  authority?: string;
  caCert?: string;
  clientCert?: string;
  clientKey?: string;
  skipVerify?: boolean;
}

export const useConnectionStore = create<ConnectionState>()(
  devtools(
    persist(
      (set, get) => ({
        isConnected: false,
        isConnecting: false,
        error: null,
        address: '',
        services: [],
        currentService: null,
        currentMethod: null,

        connect: async (address: string, tls?: TLSConfig) => {
          set({ isConnecting: true, error: null });
          try {
            const result = await tauriApi.grpcConnect(address, tls);
            if (result.success) {
              set({
                isConnected: true,
                address,
                isConnecting: false,
                error: null,
              });
              await get().loadServices();
            } else {
              set({
                isConnected: false,
                isConnecting: false,
                error: result.error || 'Connection failed',
              });
            }
          } catch (err) {
            set({
              isConnected: false,
              isConnecting: false,
              error: err instanceof Error ? err.message : 'Connection failed',
            });
          }
        },

        disconnect: async () => {
          try {
            const result = await tauriApi.grpcDisconnect();
            if (!result.success) {
              console.warn('Failed to reset backend parser on disconnect:', result.error);
            }
          } catch (err) {
            console.error('Failed to disconnect from backend:', err);
          }
          set({
            isConnected: false,
            address: '',
            services: [],
            currentService: null,
            currentMethod: null,
            error: null,
          });
        },

        loadServices: async () => {
          try {
            const result = await tauriApi.grpcListServices();
            set({ services: result.services });
          } catch (err) {
            set({
              error: err instanceof Error ? err.message : 'Failed to load services',
            });
          }
        },

        selectService: (service: Service | null) => {
          set({ currentService: service, currentMethod: null });
        },

        selectMethod: (method: string | null) => {
          set({ currentMethod: method });
        },

        clearError: () => {
          set({ error: null });
        },
      }),
      {
        name: 'grpcui-connection',
        partialize: (state) => ({
          address: state.address,
        }),
      }
    ),
    { name: 'ConnectionStore' }
  )
);
