import { describe, test, expect, beforeEach } from 'vitest';
import { tauriApi } from '../../src/src/lib/tauriApi';
import type { Environment, Collection, History } from '../../src/src/types';

describe('Storage E2E - Environment CRUD', () => {
  beforeEach(async () => {
    // Clean up environments before each test
    const envs = await tauriApi.getEnvironments();
    for (const env of envs) {
      // Note: deleteEnvironment may need to be added to tauriApi
    }
  });

  test('should create and retrieve environment', async () => {
    const env: Environment = {
      id: 'test-env-1',
      name: 'Test Environment',
      baseUrl: 'localhost:50051',
      tls: {
        mode: 'insecure',
      },
      metadata: {
        'x-env': 'test',
      },
      variables: [
        { key: 'API_KEY', value: 'test-key', secret: false },
        { key: 'SECRET', value: 'hidden', secret: true },
      ],
    };

    await tauriApi.saveEnvironment(env);
    const envs = await tauriApi.getEnvironments();

    expect(envs).toContainEqual(expect.objectContaining({
      id: 'test-env-1',
      name: 'Test Environment',
    }));
  });

  test('should update environment', async () => {
    const env: Environment = {
      id: 'test-env-update',
      name: 'Original Name',
      baseUrl: 'localhost:50051',
      tls: { mode: 'insecure' },
      metadata: {},
      variables: [],
    };

    await tauriApi.saveEnvironment(env);

    const updatedEnv: Environment = {
      ...env,
      name: 'Updated Name',
      baseUrl: 'localhost:50052',
    };

    await tauriApi.saveEnvironment(updatedEnv);
    const envs = await tauriApi.getEnvironments();

    const found = envs.find(e => e.id === 'test-env-update');
    expect(found?.name).toBe('Updated Name');
    expect(found?.baseUrl).toBe('localhost:50052');
  });

  test('should handle multiple environments', async () => {
    const env1: Environment = {
      id: 'env-1',
      name: 'Environment 1',
      baseUrl: 'localhost:50051',
      tls: { mode: 'insecure' },
      metadata: {},
      variables: [],
    };

    const env2: Environment = {
      id: 'env-2',
      name: 'Environment 2',
      baseUrl: 'localhost:50052',
      tls: { mode: 'system' },
      metadata: {},
      variables: [],
    };

    await tauriApi.saveEnvironment(env1);
    await tauriApi.saveEnvironment(env2);

    const envs = await tauriApi.getEnvironments();
    expect(envs.length).toBeGreaterThanOrEqual(2);
  });
});

describe('Storage E2E - Collection CRUD', () => {
  test('should create and retrieve collection', async () => {
    const collection: Collection = {
      id: 'test-collection-1',
      name: 'Test Collection',
      folders: [],
      items: [
        {
          id: 'item-1',
          name: 'Test Request',
          type: 'unary',
          service: 'test.Greeter',
          method: 'SayHello',
          body: JSON.stringify({ name: 'test' }),
          metadata: {},
        },
      ],
    };

    await tauriApi.saveCollection(collection);
    const collections = await tauriApi.getCollections();

    expect(collections).toContainEqual(expect.objectContaining({
      id: 'test-collection-1',
      name: 'Test Collection',
    }));
  });

  test('should update collection with folders', async () => {
    const collection: Collection = {
      id: 'test-collection-folders',
      name: 'Collection with Folders',
      folders: [
        {
          id: 'folder-1',
          name: 'Folder 1',
          items: [
            {
              id: 'folder-item-1',
              name: 'Foldered Request',
              type: 'unary',
              service: 'test.Greeter',
              method: 'SayHello',
              body: '{}',
              metadata: {},
            },
          ],
        },
      ],
      items: [],
    };

    await tauriApi.saveCollection(collection);
    const collections = await tauriApi.getCollections();

    const found = collections.find(c => c.id === 'test-collection-folders');
    expect(found?.folders.length).toBe(1);
    expect(found?.folders[0].name).toBe('Folder 1');
  });

  test('should handle collection with streaming requests', async () => {
    const collection: Collection = {
      id: 'test-collection-streaming',
      name: 'Streaming Collection',
      folders: [],
      items: [
        {
          id: 'stream-item-1',
          name: 'Server Stream',
          type: 'server_stream',
          service: 'test.StreamingService',
          method: 'ServerStream',
          body: JSON.stringify({ count: 5 }),
          metadata: {},
        },
        {
          id: 'stream-item-2',
          name: 'Client Stream',
          type: 'client_stream',
          service: 'test.StreamingService',
          method: 'ClientStream',
          body: '{}',
          metadata: {},
        },
        {
          id: 'stream-item-3',
          name: 'Bidi Stream',
          type: 'bidi_stream',
          service: 'test.StreamingService',
          method: 'BidiStream',
          body: '{}',
          metadata: {},
        },
      ],
    };

    await tauriApi.saveCollection(collection);
    const collections = await tauriApi.getCollections();

    const found = collections.find(c => c.id === 'test-collection-streaming');
    expect(found?.items.length).toBe(3);
    expect(found?.items.map(i => i.type)).toContain('server_stream');
    expect(found?.items.map(i => i.type)).toContain('client_stream');
    expect(found?.items.map(i => i.type)).toContain('bidi_stream');
  });
});

describe('Storage E2E - History', () => {
  test('should add and retrieve history', async () => {
    const history: History = {
      id: 'history-1',
      timestamp: Date.now(),
      service: 'test.Greeter',
      method: 'SayHello',
      address: 'localhost:50051',
      status: 'success',
      duration: 150,
      requestSnapshot: {
        id: 'snap-1',
        name: 'Snapshot Request',
        type: 'unary',
        service: 'test.Greeter',
        method: 'SayHello',
        body: JSON.stringify({ name: 'history-test' }),
        metadata: { 'x-test': 'value' },
      },
    };

    await tauriApi.addHistory(history);
    const histories = await tauriApi.getHistories();

    expect(histories).toContainEqual(expect.objectContaining({
      id: 'history-1',
      service: 'test.Greeter',
      method: 'SayHello',
    }));
  });

  test('should retrieve history with limit', async () => {
    // Add multiple history entries
    for (let i = 0; i < 5; i++) {
      const history: History = {
        id: `history-limit-${i}`,
        timestamp: Date.now() + i,
        service: 'test.Greeter',
        method: 'SayHello',
        address: 'localhost:50051',
        status: 'success',
        duration: 100 + i,
        requestSnapshot: {
          id: `snap-${i}`,
          name: `Request ${i}`,
          type: 'unary',
          service: 'test.Greeter',
          method: 'SayHello',
          body: '{}',
          metadata: {},
        },
      };
      await tauriApi.addHistory(history);
    }

    const histories = await tauriApi.getHistories(3);
    expect(histories.length).toBeLessThanOrEqual(3);
  });

  test('should store error history', async () => {
    const history: History = {
      id: 'history-error',
      timestamp: Date.now(),
      service: 'test.Greeter',
      method: 'SayHello',
      address: 'localhost:59999',
      status: 'error',
      duration: 50,
      requestSnapshot: {
        id: 'snap-error',
        name: 'Error Request',
        type: 'unary',
        service: 'test.Greeter',
        method: 'SayHello',
        body: '{}',
        metadata: {},
      },
    };

    await tauriApi.addHistory(history);
    const histories = await tauriApi.getHistories();

    const found = histories.find(h => h.id === 'history-error');
    expect(found?.status).toBe('error');
  });

  test('should store streaming history', async () => {
    const history: History = {
      id: 'history-streaming',
      timestamp: Date.now(),
      service: 'test.StreamingService',
      method: 'ServerStream',
      address: 'localhost:50051',
      status: 'success',
      duration: 2000,
      requestSnapshot: {
        id: 'snap-streaming',
        name: 'Streaming Request',
        type: 'server_stream',
        service: 'test.StreamingService',
        method: 'ServerStream',
        body: JSON.stringify({ count: 10 }),
        metadata: {},
      },
    };

    await tauriApi.addHistory(history);
    const histories = await tauriApi.getHistories();

    const found = histories.find(h => h.id === 'history-streaming');
    expect(found?.requestSnapshot.type).toBe('server_stream');
  });
});
