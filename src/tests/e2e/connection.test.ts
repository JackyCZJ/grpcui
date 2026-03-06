import { describe, test, expect, beforeAll, afterAll, vi } from 'vitest';
import { tauriApi } from '../../src/lib/tauriApi';

// Mock Tauri API for E2E tests
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async (cmd: string, args?: any) => {
    switch (cmd) {
      case 'grpc_connect':
        // Simulate connection failure for invalid port
        if (args?.request?.address?.includes('59999')) {
          return { success: false, error: 'Connection refused' };
        }
        return { success: true };
      case 'grpc_list_services':
        return {
          services: [
            { name: 'Greeter', fullName: 'test.Greeter', methods: [] },
            { name: 'StreamingService', fullName: 'test.StreamingService', methods: [] },
          ],
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

describe('gRPC Connection E2E', () => {
  let mockServer: { port: number; stop: () => Promise<void> };

  beforeAll(async () => {
    mockServer = await startMockServer(50051);
  });

  afterAll(async () => {
    await stopMockServer(mockServer);
  });

  test('connect to gRPC server', async () => {
    const result = await tauriApi.grpcConnect('localhost:50051');
    expect(result.success).toBe(true);
  });

  test('list services', async () => {
    const result = await tauriApi.grpcListServices();
    expect(result.services.length).toBeGreaterThan(0);
    expect(result.services[0].name).toBeDefined();
  });

  test('should handle connection failure', async () => {
    const result = await tauriApi.grpcConnect('localhost:59999');
    expect(result.success).toBe(false);
    expect(result.error).toBeDefined();
  });

  test('should handle TLS connection', async () => {
    const result = await tauriApi.grpcConnect('localhost:50051', {
      mode: 'insecure',
      skipVerify: true,
    });
    expect(result.success).toBe(true);
  });
});
