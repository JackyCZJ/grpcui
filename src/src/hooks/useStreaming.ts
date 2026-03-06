import { useState, useCallback, useRef, useEffect } from 'react';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { tauriApi } from '../lib/tauriApi';
import type { StreamMessage } from '../types';

interface StreamState {
  messages: StreamMessage[];
  isStreaming: boolean;
  error: string | null;
}

interface StreamActions {
  start: (method: string, body: string, metadata?: Record<string, string>) => Promise<void>;
  stop: () => void;
  sendMessage: (message: unknown) => void;
  clearMessages: () => void;
}

/**
 * Hook for server-side streaming gRPC calls
 * Server streams messages to client
 */
export function useServerStream(): StreamState & StreamActions {
  const [messages, setMessages] = useState<StreamMessage[]>([]);
  const [isStreaming, setIsStreaming] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const streamIdRef = useRef<string | null>(null);
  const unlistenRef = useRef<UnlistenFn | null>(null);

  const start = useCallback(
    async (method: string, body: string, metadata?: Record<string, string>) => {
      try {
        setIsStreaming(true);
        setError(null);
        setMessages([]);

        const streamId = await tauriApi.grpcInvokeStream({
          method,
          body,
          metadata,
        });

        streamIdRef.current = streamId;

        // Listen for stream events
        const unlisten = await listen<StreamMessage>(
          `grpc-stream-${streamId}`,
          (event) => {
            const message = event.payload;
            setMessages((prev) => [...prev, message]);

            if (message.type === 'end' || message.type === 'error') {
              setIsStreaming(false);
            }
          }
        );

        unlistenRef.current = unlisten;
      } catch (err) {
        setError(err instanceof Error ? err.message : 'Failed to start stream');
        setIsStreaming(false);
      }
    },
    []
  );

  const stop = useCallback(() => {
    if (unlistenRef.current) {
      unlistenRef.current();
      unlistenRef.current = null;
    }
    streamIdRef.current = null;
    setIsStreaming(false);
  }, []);

  const sendMessage = useCallback(() => {
    // Server stream doesn't support client sending messages
    console.warn('Cannot send messages in server-side streaming mode');
  }, []);

  const clearMessages = useCallback(() => {
    setMessages([]);
  }, []);

  useEffect(() => {
    return () => {
      if (unlistenRef.current) {
        unlistenRef.current();
      }
    };
  }, []);

  return {
    messages,
    isStreaming,
    error,
    start,
    stop,
    sendMessage,
    clearMessages,
  };
}

/**
 * Hook for client-side streaming gRPC calls
 * Client streams messages to server
 */
export function useClientStream(): StreamState &
  Omit<StreamActions, 'sendMessage'> & {
    sendMessage: (message: unknown) => Promise<void>;
  } {
  const [messages, setMessages] = useState<StreamMessage[]>([]);
  const [isStreaming, setIsStreaming] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const streamIdRef = useRef<string | null>(null);
  const unlistenRef = useRef<UnlistenFn | null>(null);

  const start = useCallback(
    async (method: string, body: string, metadata?: Record<string, string>) => {
      try {
        setIsStreaming(true);
        setError(null);
        setMessages([]);

        const streamId = await tauriApi.grpcInvokeStream({
          method,
          body,
          metadata,
        });

        streamIdRef.current = streamId;

        const unlisten = await listen<StreamMessage>(
          `grpc-stream-${streamId}`,
          (event) => {
            const message = event.payload;
            setMessages((prev) => [...prev, message]);

            if (message.type === 'end' || message.type === 'error') {
              setIsStreaming(false);
            }
          }
        );

        unlistenRef.current = unlisten;
      } catch (err) {
        setError(err instanceof Error ? err.message : 'Failed to start stream');
        setIsStreaming(false);
      }
    },
    []
  );

  const stop = useCallback(() => {
    if (unlistenRef.current) {
      unlistenRef.current();
      unlistenRef.current = null;
    }
    streamIdRef.current = null;
    setIsStreaming(false);
  }, []);

  const sendMessage = useCallback(
    async (message: unknown) => {
      if (!streamIdRef.current) {
        console.warn('No active stream');
        return;
      }

      const { emit } = await import('@tauri-apps/api/event');
      await emit(`grpc-stream-send-${streamIdRef.current}`, {
        message,
      });
    },
    []
  );

  const clearMessages = useCallback(() => {
    setMessages([]);
  }, []);

  useEffect(() => {
    return () => {
      if (unlistenRef.current) {
        unlistenRef.current();
      }
    };
  }, []);

  return {
    messages,
    isStreaming,
    error,
    start,
    stop,
    sendMessage,
    clearMessages,
  };
}

/**
 * Hook for bidirectional streaming gRPC calls
 * Both client and server can send messages
 */
export function useBidiStream(): StreamState &
  Omit<StreamActions, 'sendMessage'> & {
    sendMessage: (message: unknown) => Promise<void>;
  } {
  const [messages, setMessages] = useState<StreamMessage[]>([]);
  const [isStreaming, setIsStreaming] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const streamIdRef = useRef<string | null>(null);
  const unlistenRef = useRef<UnlistenFn | null>(null);

  const start = useCallback(
    async (method: string, body: string, metadata?: Record<string, string>) => {
      try {
        setIsStreaming(true);
        setError(null);
        setMessages([]);

        const streamId = await tauriApi.grpcInvokeStream({
          method,
          body,
          metadata,
        });

        streamIdRef.current = streamId;

        const unlisten = await listen<StreamMessage>(
          `grpc-stream-${streamId}`,
          (event) => {
            const message = event.payload;
            setMessages((prev) => [...prev, message]);

            if (message.type === 'end' || message.type === 'error') {
              setIsStreaming(false);
            }
          }
        );

        unlistenRef.current = unlisten;
      } catch (err) {
        setError(err instanceof Error ? err.message : 'Failed to start stream');
        setIsStreaming(false);
      }
    },
    []
  );

  const stop = useCallback(() => {
    if (unlistenRef.current) {
      unlistenRef.current();
      unlistenRef.current = null;
    }
    streamIdRef.current = null;
    setIsStreaming(false);
  }, []);

  const sendMessage = useCallback(async (message: unknown) => {
    if (!streamIdRef.current) {
      console.warn('No active stream');
      return;
    }

    const { emit } = await import('@tauri-apps/api/event');
    await emit(`grpc-stream-send-${streamIdRef.current}`, {
      message,
    });
  }, []);

  const clearMessages = useCallback(() => {
    setMessages([]);
  }, []);

  useEffect(() => {
    return () => {
      if (unlistenRef.current) {
        unlistenRef.current();
      }
    };
  }, []);

  return {
    messages,
    isStreaming,
    error,
    start,
    stop,
    sendMessage,
    clearMessages,
  };
}

/**
 * Generic streaming hook that selects the appropriate stream type
 * based on the method type
 */
export function useStreaming(methodType: 'server_stream' | 'client_stream' | 'bidi_stream') {
  const serverStream = useServerStream();
  const clientStream = useClientStream();
  const bidiStream = useBidiStream();

  switch (methodType) {
    case 'server_stream':
      return serverStream;
    case 'client_stream':
      return clientStream;
    case 'bidi_stream':
      return bidiStream;
    default:
      return serverStream;
  }
}
