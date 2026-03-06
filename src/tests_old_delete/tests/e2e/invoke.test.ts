import { describe, test, expect, beforeAll, afterAll } from 'vitest';
import { tauriApi } from '../../src/src/lib/tauriApi';
import { startMockServer, stopMockServer, MockGrpcServer } from '../mocks/grpc-server';

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
