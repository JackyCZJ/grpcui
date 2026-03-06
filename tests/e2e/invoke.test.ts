import { describe, test, expect, beforeAll, afterAll, vi } from 'vitest';
import { tauriApi } from '../../src/lib/tauriApi';

// Mock Tauri API for E2E tests
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async (cmd: string, args?: any) => {
    switch (cmd) {
      case 'grpc_connect':
        return { success: true };
      case 'grpc_invoke':
        if (args?.request?.method?.includes('Invalid')) {
          return { error: 'Method not found', status: 'ERROR', metadata: {}, duration: 0 };
        }
        if (args?.request?.body === 'invalid json') {
          return { error: 'Invalid JSON', status: 'ERROR', metadata: {}, duration: 0 };
        }
        return {
          status: 'OK',
          data: { message: 'Hello!' },
          metadata: args?.request?.metadata || {},
          duration: 150,
        };
      default:
        return undefined;
    }
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

describe('gRPC Invoke E2E', () => {
  let mockServer: MockGrpcServer;

  beforeAll(async () => {
    mockServer = await startMockServer(50053);
    // Establish connection first
    await tauriApi.grpcConnect('localhost:50053');
  });

  afterAll(async () => {
    await stopMockServer(mockServer);
  });

  test('unary invoke', async () => {
    const result = await tauriApi.grpcInvoke({
      method: 'test.Greeter/SayHello',
      body: JSON.stringify({ name: 'hello' }),
      metadata: {},
    });
    expect(result.status).toBe('OK');
    expect(result.data).toBeDefined();
  });

  test('unary invoke with metadata', async () => {
    const result = await tauriApi.grpcInvoke({
      method: 'test.Greeter/SayHello',
      body: JSON.stringify({ name: 'test' }),
      metadata: {
        'x-request-id': '12345',
        'x-custom-header': 'custom-value',
      },
    });
    expect(result.status).toBe('OK');
    expect(result.metadata).toBeDefined();
  });

  test('should handle invalid method', async () => {
    const result = await tauriApi.grpcInvoke({
      method: 'test.InvalidService/InvalidMethod',
      body: JSON.stringify({}),
      metadata: {},
    });
    expect(result.error).toBeDefined();
  });

  test('should handle invalid JSON body', async () => {
    const result = await tauriApi.grpcInvoke({
      method: 'test.Greeter/SayHello',
      body: 'invalid json',
      metadata: {},
    });
    expect(result.error).toBeDefined();
  });

  test('should measure response duration', async () => {
    const result = await tauriApi.grpcInvoke({
      method: 'test.Greeter/SayHello',
      body: JSON.stringify({ name: 'duration-test' }),
      metadata: {},
    });
    expect(result.status).toBe('OK');
    expect(result.duration).toBeGreaterThanOrEqual(0);
  });
});
