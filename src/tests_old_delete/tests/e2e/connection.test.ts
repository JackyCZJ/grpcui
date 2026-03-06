import { describe, test, expect, beforeAll, afterAll } from 'vitest';
import { tauriApi } from '../../src/src/lib/tauriApi';
import { startMockServer, stopMockServer, MockGrpcServer } from '../mocks/grpc-server';

describe('gRPC Connection E2E', () => {
  let mockServer: MockGrpcServer;

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
