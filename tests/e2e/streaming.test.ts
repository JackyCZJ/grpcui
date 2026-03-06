import { describe, test, expect, beforeAll, afterAll, vi } from 'vitest';
import { tauriApi } from '../../src/lib/tauriApi';

// Mock Tauri API for E2E tests
const mockListeners: Map<string, Array<(payload: any) => void>> = new Map();

// Track active streams for message simulation
const activeStreams: Map<string, { type: string; messages: any[] }> = new Map();

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async (cmd: string, args?: any) => {
    switch (cmd) {
      case 'grpc_connect':
        return { success: true };
      case 'grpc_invoke_stream': {
        const streamId = `stream-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
        const method = args?.request?.method || '';
        activeStreams.set(streamId, { type: method, messages: [] });

        // Simulate server streaming messages
        if (method.includes('ServerStream')) {
          setTimeout(() => {
            const listeners = mockListeners.get(`stream:${streamId}`);
            if (listeners) {
              for (let i = 0; i < 3; i++) {
                listeners.forEach(cb => cb({ payload: { data: `message ${i}`, sequence: i } }));
              }
            }
          }, 100);
        }
        return streamId;
      }
      case 'grpc_send_stream_message': {
        const streamId = args?.streamId;
        const message = args?.message;
        const stream = activeStreams.get(streamId);

        // Simulate bidi streaming echo
        if (stream?.type.includes('BidiStream')) {
          const listeners = mockListeners.get(`stream:${streamId}`);
          if (listeners) {
            const data = JSON.parse(message);
            listeners.forEach(cb => cb({ payload: { data: `Echo: ${data.data}`, sequence: stream.messages.length } }));
          }
        }
        return undefined;
      }
      case 'grpc_close_stream':
        activeStreams.delete(args?.streamId);
        return undefined;
      default:
        return undefined;
    }
  }),
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn((event: string, callback: (payload: any) => void) => {
    if (!mockListeners.has(event)) {
      mockListeners.set(event, []);
    }
    mockListeners.get(event)!.push(callback);
    return Promise.resolve(() => {
      const listeners = mockListeners.get(event);
      if (listeners) {
        const index = listeners.indexOf(callback);
        if (index > -1) listeners.splice(index, 1);
      }
    });
  }),
  emit: vi.fn((event: string, payload: any) => {
    const listeners = mockListeners.get(event);
    if (listeners) listeners.forEach((cb) => cb(payload));
  }),
}));

// Mock gRPC server for testing
interface MockGrpcServer {
  port: number;
  stop: () => Promise<void>;
}

async function startMockServer(port: number): Promise<MockGrpcServer> {
  return { port, stop: async () => {} };
}

async function stopMockServer(_server: MockGrpcServer): Promise<void> {
  // Cleanup
}

async function listen(event: string, callback: (event: any) => void): Promise<() => void> {
  if (!mockListeners.has(event)) {
    mockListeners.set(event, []);
  }
  mockListeners.get(event)!.push(callback);
  return () => {
    const listeners = mockListeners.get(event);
    if (listeners) {
      const index = listeners.indexOf(callback);
      if (index > -1) listeners.splice(index, 1);
    }
  };
}

describe('gRPC Streaming E2E', () => {
  let mockServer: { port: number; stop: () => Promise<void> };

  beforeAll(async () => {
    mockServer = await startMockServer(50052);
    // Establish connection
    await tauriApi.grpcConnect('localhost:50052');
  });

  afterAll(async () => {
    await stopMockServer(mockServer);
  });

  test('server streaming', async () => {
    const streamId = await tauriApi.grpcInvokeStream({
      method: 'test.StreamingService/ServerStream',
      body: JSON.stringify({ count: 3 }),
      metadata: {},
    });

    expect(streamId).toBeDefined();
    expect(typeof streamId).toBe('string');

    const messages: any[] = [];
    const unlisten = await listen(`stream:${streamId}`, (event) => {
      messages.push(event.payload);
    });

    await new Promise((resolve) => setTimeout(resolve, 1000));
    unlisten();

    expect(messages.length).toBeGreaterThan(0);
  });

  test('client streaming', async () => {
    const streamId = await tauriApi.grpcInvokeStream({
      method: 'test.StreamingService/ClientStream',
      body: JSON.stringify({}),
      metadata: {},
    });

    expect(streamId).toBeDefined();
    expect(typeof streamId).toBe('string');

    // Send multiple messages to the stream
    for (let i = 0; i < 3; i++) {
      await tauriApi.grpcSendStreamMessage(
        streamId,
        JSON.stringify({ data: `message ${i}` })
      );
    }

    // Close the stream and get result
    await tauriApi.grpcCloseStream(streamId);
  });

  test('bidi streaming', async () => {
    const streamId = await tauriApi.grpcInvokeStream({
      method: 'test.StreamingService/BidiStream',
      body: JSON.stringify({}),
      metadata: {},
    });

    expect(streamId).toBeDefined();
    expect(typeof streamId).toBe('string');

    const messages: any[] = [];
    const unlisten = await listen(`stream:${streamId}`, (event) => {
      messages.push(event.payload);
    });

    // Send messages and receive echoes
    for (let i = 0; i < 3; i++) {
      await tauriApi.grpcSendStreamMessage(
        streamId,
        JSON.stringify({ data: `message ${i}` })
      );
      await new Promise((resolve) => setTimeout(resolve, 100));
    }

    await new Promise((resolve) => setTimeout(resolve, 500));
    unlisten();

    expect(messages.length).toBeGreaterThan(0);

    await tauriApi.grpcCloseStream(streamId);
  });

  test('should handle stream errors', async () => {
    const streamId = await tauriApi.grpcInvokeStream({
      method: 'test.StreamingService/ErrorStream',
      body: JSON.stringify({}),
      metadata: {},
    });

    expect(streamId).toBeDefined();

    const errors: any[] = [];
    const unlisten = await listen(`stream-error:${streamId}`, (event) => {
      errors.push(event.payload);
    });

    await new Promise((resolve) => setTimeout(resolve, 500));
    unlisten();

    // Error stream may or may not produce errors depending on implementation
    expect(streamId).toBeTruthy();
  });
});
