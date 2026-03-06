import { vi } from 'vitest'

// Mock Tauri API for E2E tests
const mockInvoke = vi.fn(async (cmd: string, args?: any) => {
  // Simulate backend responses based on command
  switch (cmd) {
    case 'grpc_connect':
      return { success: true }
    case 'grpc_list_services':
      return {
        services: [
          {
            name: 'Greeter',
            fullName: 'test.Greeter',
            methods: [
              {
                name: 'SayHello',
                fullName: 'test.Greeter.SayHello',
                inputType: 'HelloRequest',
                outputType: 'HelloReply',
                type: 'unary',
              },
            ],
          },
          {
            name: 'StreamingService',
            fullName: 'test.StreamingService',
            methods: [
              {
                name: 'ServerStream',
                fullName: 'test.StreamingService.ServerStream',
                inputType: 'StreamRequest',
                outputType: 'StreamResponse',
                type: 'server_stream',
              },
              {
                name: 'ClientStream',
                fullName: 'test.StreamingService.ClientStream',
                inputType: 'StreamRequest',
                outputType: 'StreamResponse',
                type: 'client_stream',
              },
              {
                name: 'BidiStream',
                fullName: 'test.StreamingService.BidiStream',
                inputType: 'StreamRequest',
                outputType: 'StreamResponse',
                type: 'bidi_stream',
              },
            ],
          },
        ],
      }
    case 'grpc_invoke':
      return {
        status: 'OK',
        data: { message: 'Hello, test!' },
        metadata: {},
        duration: 150,
      }
    case 'grpc_invoke_stream':
      return `stream-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`
    case 'grpc_send_stream_message':
      return undefined
    case 'grpc_close_stream':
      return undefined
    case 'get_environments':
      return []
    case 'save_environment':
      return undefined
    case 'get_collections':
      return []
    case 'save_collection':
      return undefined
    case 'get_histories':
      return []
    case 'add_history':
      return undefined
    default:
      console.warn(`Unhandled mock command: ${cmd}`)
      return undefined
  }
})

vi.mock('@tauri-apps/api/core', () => ({
  invoke: mockInvoke,
}))

// Mock event listener for streaming
const mockListeners: Map<string, Array<(payload: any) => void>> = new Map()

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn((event: string, callback: (payload: any) => void) => {
    if (!mockListeners.has(event)) {
      mockListeners.set(event, [])
    }
    mockListeners.get(event)!.push(callback)

    // Return unlisten function
    return Promise.resolve(() => {
      const listeners = mockListeners.get(event)
      if (listeners) {
        const index = listeners.indexOf(callback)
        if (index > -1) {
          listeners.splice(index, 1)
        }
      }
    })
  }),
  emit: vi.fn((event: string, payload: any) => {
    const listeners = mockListeners.get(event)
    if (listeners) {
      listeners.forEach((cb) => cb(payload))
    }
  }),
}))

// Global test timeout
vi.setConfig({ testTimeout: 60000 })
