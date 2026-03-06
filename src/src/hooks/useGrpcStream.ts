import { useState, useCallback, useRef, useEffect } from 'react';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { tauriApi } from '../lib/tauriApi';
import type { MethodType, TLSConfig } from '../types';

// ============ Types ============

export interface UseGrpcStreamOptions {
  /** Callback when a message is received from the stream */
  onMessage?: (message: unknown) => void;
  /** Callback when stream metadata is received */
  onMetadata?: (metadata: Record<string, string>) => void;
  /** Callback when an error occurs */
  onError?: (error: string) => void;
  /** Callback when the stream is closed */
  onClose?: () => void;
}

export interface UseGrpcStreamReturn {
  /** Whether the stream is currently connected/active */
  isConnected: boolean;
  /** Array of all received messages */
  messages: unknown[];
  /** Connect to a gRPC stream */
  connect: (
    address: string,
    method: string,
    body?: string,
    metadata?: Record<string, string>,
    streamType?: 'client' | 'server' | 'bidi',
    tls?: TLSConfig
  ) => Promise<void>;
  /** Send a message to the stream (for client-stream or bidi-stream) */
  sendMessage: (message: unknown) => Promise<void>;
  /** Half-close stream input (for client-stream or bidi-stream) */
  end: () => Promise<void>;
  /** Close the stream connection */
  close: () => Promise<void>;
  /** Clear all accumulated messages */
  clearMessages: () => void;
}

interface StreamEventPayload {
  type: 'message' | 'error' | 'end' | 'status' | 'metadata';
  data?: unknown;
  payload?: unknown;
  error?: string;
  message?: string;
  metadata?: Record<string, string>;
}

// ============ Hook Implementation ============

/**
 * Unified hook for handling gRPC streaming calls.
 */
export function useGrpcStream(options?: UseGrpcStreamOptions): UseGrpcStreamReturn {
  const [isConnected, setIsConnected] = useState(false);
  const [messages, setMessages] = useState<unknown[]>([]);

  const streamIdRef = useRef<string | null>(null);
  const unlistenRef = useRef<UnlistenFn | null>(null);
  const optionsRef = useRef(options);

  useEffect(() => {
    optionsRef.current = options;
  }, [options]);

  // cleanup 用于释放监听器和本地流状态，
  // 避免用户反复切换方法或项目时产生悬挂连接。
  const cleanup = useCallback(() => {
    if (unlistenRef.current) {
      unlistenRef.current();
      unlistenRef.current = null;
    }
    streamIdRef.current = null;
    setIsConnected(false);
  }, []);

  // connect 会根据流类型发起 sidecar 调用，
  // 并统一监听后端推送的 grpc:stream 事件。
  const connect = useCallback(
    async (
      address: string,
      method: string,
      body = '{}',
      metadata?: Record<string, string>,
      streamType: 'client' | 'server' | 'bidi' = 'server',
      tls?: TLSConfig
    ) => {
      cleanup();

      try {
        const streamId = await tauriApi.grpcInvokeStream({
          method,
          body,
          metadata,
          address,
          streamType,
          tls,
        });

        streamIdRef.current = streamId;
        setIsConnected(true);

        const unlisten = await listen<StreamEventPayload>(
          `grpc:stream:${streamId}`,
          (event) => {
            const payload = event.payload;
            const eventType = payload.type;

            switch (eventType) {
              case 'message': {
                const message = payload.data ?? payload.payload;
                setMessages((prev) => [...prev, message]);
                optionsRef.current?.onMessage?.(message);
                break;
              }

              case 'error': {
                setIsConnected(false);
                const errorMessage = payload.error ?? payload.message ?? 'Unknown stream error';
                optionsRef.current?.onError?.(errorMessage);
                break;
              }

              case 'metadata': {
                const metadata = payload.metadata ?? {};
                optionsRef.current?.onMetadata?.(metadata);
                break;
              }

              case 'end':
                setIsConnected(false);
                optionsRef.current?.onClose?.();
                break;

              case 'status':
              default:
                break;
            }
          }
        );

        unlistenRef.current = unlisten;
      } catch (err) {
        const errorMessage = err instanceof Error ? err.message : 'Failed to connect to stream';
        optionsRef.current?.onError?.(errorMessage);
        cleanup();
        throw err;
      }
    },
    [cleanup]
  );

  // sendMessage 通过 Tauri 命令向活动流发送增量消息，
  // 主要用于后续支持的 client/bidi 交互场景。
  const sendMessage = useCallback(async (message: unknown) => {
    if (!streamIdRef.current) {
      const error = 'Cannot send message: no active stream';
      optionsRef.current?.onError?.(error);
      throw new Error(error);
    }

    try {
      await tauriApi.grpcSendStreamMessage(streamIdRef.current, JSON.stringify(message));
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : 'Failed to send message';
      optionsRef.current?.onError?.(errorMessage);
      throw err;
    }
  }, []);

  // end 会向后端发送 half-close 信号，
  // 让 client/bidi 流在不强制断开连接的前提下返回最终结果。
  const end = useCallback(async () => {
    if (!streamIdRef.current) {
      return;
    }

    try {
      await tauriApi.grpcEndStream(streamIdRef.current);
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : 'Failed to end stream';
      optionsRef.current?.onError?.(errorMessage);
      throw err;
    }
  }, []);

  // close 会主动关闭后端流并清理本地状态，
  // 保证 UI 在离开页面时不会继续消费无效事件。
  const close = useCallback(async () => {
    if (!streamIdRef.current) {
      return;
    }

    try {
      await tauriApi.grpcCloseStream(streamIdRef.current);
    } catch {
      // Ignore errors during close, just clean up local state.
    } finally {
      cleanup();
      optionsRef.current?.onClose?.();
    }
  }, [cleanup]);

  const clearMessages = useCallback(() => {
    setMessages([]);
  }, []);

  useEffect(() => {
    return () => {
      cleanup();
    };
  }, [cleanup]);

  return {
    isConnected,
    messages,
    connect,
    sendMessage,
    end,
    close,
    clearMessages,
  };
}

export function useGrpcServerStream(options?: UseGrpcStreamOptions): UseGrpcStreamReturn {
  const stream = useGrpcStream(options);

  return {
    ...stream,
    connect: (address, method, body, metadata, _streamType, tls) =>
      stream.connect(address, method, body, metadata, 'server', tls),
    sendMessage: async () => {
      console.warn('Server streaming does not support sending additional messages');
    },
    end: async () => {
      console.warn('Server streaming does not support half-close');
    },
  };
}

export function useGrpcClientStream(options?: UseGrpcStreamOptions): UseGrpcStreamReturn {
  const stream = useGrpcStream(options);
  return {
    ...stream,
    connect: (address, method, body, metadata, _streamType, tls) =>
      stream.connect(address, method, body, metadata, 'client', tls),
  };
}

export function useGrpcBidiStream(options?: UseGrpcStreamOptions): UseGrpcStreamReturn {
  const stream = useGrpcStream(options);
  return {
    ...stream,
    connect: (address, method, body, metadata, _streamType, tls) =>
      stream.connect(address, method, body, metadata, 'bidi', tls),
  };
}

export function useGrpcStreamByType(
  methodType: MethodType,
  options?: UseGrpcStreamOptions
): UseGrpcStreamReturn {
  const serverStream = useGrpcServerStream(options);
  const clientStream = useGrpcClientStream(options);
  const bidiStream = useGrpcBidiStream(options);

  switch (methodType) {
    case 'server_stream':
      return serverStream;
    case 'client_stream':
      return clientStream;
    case 'bidi_stream':
      return bidiStream;
    case 'unary':
    default:
      return useGrpcStream(options);
  }
}
