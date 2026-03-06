const grpc = require('@grpc/grpc-js')
const protoLoader = require('@grpc/proto-loader')
import * as path from 'path'
import * as fs from 'fs'
import * as os from 'os'

const testProtoDefinition = `
syntax = "proto3";

package test;

service Greeter {
  rpc SayHello (HelloRequest) returns (HelloReply);
}

service StreamingService {
  rpc ServerStream (StreamRequest) returns (stream StreamResponse);
  rpc ClientStream (stream StreamRequest) returns (StreamResponse);
  rpc BidiStream (stream StreamRequest) returns (stream StreamResponse);
  rpc ErrorStream (StreamRequest) returns (stream StreamResponse);
}

message HelloRequest {
  string name = 1;
}

message HelloReply {
  string message = 1;
}

message StreamRequest {
  string data = 1;
  int32 count = 2;
}

message StreamResponse {
  string data = 1;
  int32 sequence = 2;
}
`

export interface MockGrpcServer {
  server: grpc.Server
  port: number
}

function createTempProtoFile(): string {
  const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), 'grpc-test-'))
  const protoPath = path.join(tempDir, 'test.proto')
  fs.writeFileSync(protoPath, testProtoDefinition)
  return protoPath
}

export async function startMockServer(port: number = 0): Promise<MockGrpcServer> {
  const server = new grpc.Server()
  const protoPath = createTempProtoFile()

  const packageDefinition = protoLoader.loadSync(protoPath, {
    keepCase: true,
    longs: String,
    enums: String,
    defaults: true,
    oneofs: true,
  })

  const proto = grpc.loadPackageDefinition(packageDefinition) as any

  server.addService(proto.test.Greeter.service, {
    sayHello: (call: any, callback: any) => {
      callback(null, { message: `Hello, ${call.request.name}!` })
    },
  })

  server.addService(proto.test.StreamingService.service, {
    serverStream: (call: any) => {
      const count = call.request.count || 3
      for (let i = 0; i < count; i++) {
        call.write({ data: `message ${i}`, sequence: i })
      }
      call.end()
    },

    clientStream: (call: any, callback: any) => {
      let messageCount = 0
      call.on('data', () => {
        messageCount++
      })
      call.on('end', () => {
        callback(null, { data: `Received ${messageCount} messages`, sequence: messageCount })
      })
    },

    bidiStream: (call: any) => {
      let sequence = 0
      call.on('data', (request: any) => {
        call.write({ data: `Echo: ${request.data}`, sequence: sequence++ })
      })
      call.on('end', () => {
        call.end()
      })
    },

    errorStream: (call: any) => {
      call.emit('error', new Error('Test error'))
      call.end()
    },
  })

  return new Promise((resolve, reject) => {
    server.bindAsync(
      `0.0.0.0:${port}`,
      grpc.ServerCredentials.createInsecure(),
      (err, boundPort) => {
        if (err) {
          reject(err)
          return
        }
        server.start()
        resolve({ server, port: boundPort })
      }
    )
  })
}

export async function stopMockServer(mockServer: MockGrpcServer): Promise<void> {
  return new Promise((resolve) => {
    mockServer.server.tryShutdown(() => {
      resolve()
    })
  })
}

export async function cleanupMockServer(): Promise<void> {
  // Cleanup temp files if needed
}
