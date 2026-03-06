import { create } from 'zustand';
import { devtools, persist } from 'zustand/middleware';
import type { RequestItem, History, Collection, GrpcResponse } from '../types';
import { tauriApi } from '../lib/tauriApi';

interface RequestState {
  currentRequest: RequestItem | null;
  currentResponse: GrpcResponse | null;
  isLoading: boolean;
  history: History[];
  collections: Collection[];

  setRequest: (request: RequestItem | null) => void;
  updateRequestBody: (body: string) => void;
  updateRequestMetadata: (metadata: Record<string, string>) => void;
  sendRequest: () => Promise<GrpcResponse | null>;
  saveToHistory: (entry: History) => Promise<void>;
  loadHistory: (limit?: number) => Promise<void>;
  clearHistory: () => Promise<void>;
  saveToCollection: (collection: Collection) => Promise<void>;
  loadCollections: () => Promise<void>;
  deleteCollection: (id: string) => Promise<void>;
  createRequest: (service: string, method: string, methodType: RequestItem['type']) => void;
  clearResponse: () => void;
}

export const useRequestStore = create<RequestState>()(
  devtools(
    persist(
      (set, get) => ({
        currentRequest: null,
        currentResponse: null,
        isLoading: false,
        history: [],
        collections: [],

        setRequest: (request: RequestItem | null) => {
          set({ currentRequest: request, currentResponse: null });
        },

        updateRequestBody: (body: string) => {
          const { currentRequest } = get();
          if (currentRequest) {
            set({
              currentRequest: { ...currentRequest, body },
            });
          }
        },

        updateRequestMetadata: (metadata: Record<string, string>) => {
          const { currentRequest } = get();
          if (currentRequest) {
            set({
              currentRequest: { ...currentRequest, metadata },
            });
          }
        },

        sendRequest: async () => {
          const { currentRequest } = get();
          if (!currentRequest) return null;

          set({ isLoading: true, currentResponse: null });
          try {
            const response = await tauriApi.grpcInvoke({
              method: `${currentRequest.service}/${currentRequest.method}`,
              body: currentRequest.body,
              metadata: currentRequest.metadata,
            });

            set({ currentResponse: response, isLoading: false });

            const historyEntry: History = {
              id: crypto.randomUUID(),
              timestamp: Date.now(),
              service: currentRequest.service,
              method: currentRequest.method,
              address: '',
              status: response.error ? 'error' : 'success',
              responseCode: response.code,
              responseMessage: response.message,
              duration: response.duration,
              requestSnapshot: { ...currentRequest },
            };

            await get().saveToHistory(historyEntry);
            return response;
          } catch (err) {
            const errorResponse: GrpcResponse = {
              data: null,
              error: err instanceof Error ? err.message : 'Request failed',
              metadata: {},
              duration: 0,
              status: 'ERROR',
              code: -1,
              message: err instanceof Error ? err.message : 'Request failed',
            };
            set({ currentResponse: errorResponse, isLoading: false });
            return errorResponse;
          }
        },

        saveToHistory: async (entry: History) => {
          try {
            await tauriApi.addHistory(entry);
            set((state) => ({
              history: [entry, ...state.history].slice(0, 100),
            }));
          } catch (err) {
            console.error('Failed to save history:', err);
          }
        },

        loadHistory: async (limit = 50) => {
          try {
            const history = await tauriApi.getHistories(limit);
            set({ history });
          } catch (err) {
            console.error('Failed to load history:', err);
          }
        },

        clearHistory: async () => {
          set({ history: [] });
        },

        saveToCollection: async (collection: Collection) => {
          try {
            await tauriApi.saveCollection(collection);
            await get().loadCollections();
          } catch (err) {
            console.error('Failed to save collection:', err);
            throw err;
          }
        },

        loadCollections: async () => {
          try {
            const collections = await tauriApi.getCollections();
            set({ collections });
          } catch (err) {
            console.error('Failed to load collections:', err);
          }
        },

        deleteCollection: async (id: string) => {
          set((state) => ({
            collections: state.collections.filter((c) => c.id !== id),
          }));
        },

        createRequest: (service: string, method: string, methodType: RequestItem['type']) => {
          const newRequest: RequestItem = {
            id: crypto.randomUUID(),
            name: `${service.split('.').pop()}.${method}`,
            type: methodType,
            service,
            method,
            body: '{}',
            metadata: {},
          };
          set({ currentRequest: newRequest, currentResponse: null });
        },

        clearResponse: () => {
          set({ currentResponse: null });
        },
      }),
      {
        name: 'grpcui-request',
        partialize: (state) => ({
          collections: state.collections,
        }),
      }
    ),
    { name: 'RequestStore' }
  )
);
