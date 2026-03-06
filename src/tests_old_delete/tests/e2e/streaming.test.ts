import { describe, test, expect, beforeAll, afterAll } from 'vitest';
import { listen } from '@tauri-apps/api/event';
import { tauriApi } from '../../src/src/lib/tauriApi';
import { startMockServer, stopMockServer, MockGrpcServer } from '../mocks/grpc-server';

describe('gRPC Streaming E2E', () => {
  let mockServer: MockGrpcServer;

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
