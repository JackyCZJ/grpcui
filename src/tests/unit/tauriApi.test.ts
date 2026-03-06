import { beforeEach, describe, expect, it, vi } from "vitest";

const { invokeMock } = vi.hoisted(() => ({
  invokeMock: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: invokeMock,
}));

import {
  addHistory,
  clearHistories,
  deleteHistory,
  deleteEnvironment,
  getHistories,
  grpcCloseStream,
  grpcConnect,
  grpcEndStream,
  grpcInvoke,
  grpcInvokeStream,
  grpcSendStreamMessage,
} from "../../src/lib/tauriApi";

describe("grpcConnect TLS payload", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    invokeMock.mockResolvedValue({ success: true });
  });

  it("insecure 模式会带上 enabled=false，避免后端反序列化缺字段", async () => {
    await grpcConnect(
      "localhost:50051",
      { mode: "insecure" },
      { useReflection: false, protoFiles: ["user/service.proto"] }
    );

    expect(invokeMock).toHaveBeenCalledTimes(1);
    expect(invokeMock).toHaveBeenCalledWith("grpc_connect", {
      request: expect.objectContaining({
        address: "localhost:50051",
        insecure: true,
        tls: {
          enabled: false,
          insecure: true,
        },
      }),
    });
  });

  it("custom 模式会映射为后端期望的 ca_file/cert_file/key_file 字段", async () => {
    await grpcConnect(
      "localhost:50051",
      {
        mode: "custom",
        caCert: "/tmp/ca.pem",
        clientCert: "/tmp/client.pem",
        clientKey: "/tmp/client.key",
        skipVerify: true,
      },
      { useReflection: true }
    );

    expect(invokeMock).toHaveBeenCalledTimes(1);
    expect(invokeMock).toHaveBeenCalledWith("grpc_connect", {
      request: expect.objectContaining({
        insecure: false,
        use_reflection: true,
        tls: {
          enabled: true,
          ca_file: "/tmp/ca.pem",
          cert_file: "/tmp/client.pem",
          key_file: "/tmp/client.key",
          insecure: true,
        },
      }),
    });
  });
});

describe("grpcInvoke transport payload", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    invokeMock.mockResolvedValue({
      data: {},
      metadata: {},
      duration: 1,
      status: "OK",
      code: 0,
      message: "OK",
    });
  });

  it("会优先使用显式 address，authority 只走独立字段", async () => {
    await grpcInvoke({
      method: "demo.Greeter/SayHello",
      body: "{}",
      address: "127.0.0.1:50051",
      authority: "api.example.com",
      metadata: {
        token: "abc",
      },
    });

    expect(invokeMock).toHaveBeenCalledWith("grpc_invoke", {
      request: expect.objectContaining({
        address: "127.0.0.1:50051",
        authority: "api.example.com",
        metadata: {
          token: "abc",
        },
      }),
    });
  });

  it("缺少 address 时会回退默认地址，并过滤伪首部 metadata", async () => {
    await grpcInvoke({
      method: "demo.Greeter/SayHello",
      body: "{}",
      metadata: {
        ":authority": "10.0.0.8:7001",
      },
    });

    expect(invokeMock).toHaveBeenCalledWith("grpc_invoke", {
      request: expect.objectContaining({
        address: "localhost:50051",
        authority: undefined,
        metadata: undefined,
      }),
    });
  });

  it("stream 调用也会补齐默认 address，并过滤伪首部 metadata", async () => {
    invokeMock.mockResolvedValueOnce("stream-id");

    await grpcInvokeStream({
      method: "demo.Streaming/Watch",
      body: "{}",
      metadata: {
        ":authority": "stream.example:9000",
      },
    });

    expect(invokeMock).toHaveBeenCalledWith("grpc_invoke_stream", {
      request: expect.objectContaining({
        address: "localhost:50051",
        authority: undefined,
        metadata: undefined,
        stream_type: "server",
      }),
    });
  });

  it("显式 authority 会原样透传", async () => {
    await grpcInvoke({
      method: "demo.Greeter/SayHello",
      body: "{}",
      address: "192.168.1.10:50051",
      authority: "api.example.com",
    });

    expect(invokeMock).toHaveBeenCalledWith("grpc_invoke", {
      request: expect.objectContaining({
        address: "192.168.1.10:50051",
        authority: "api.example.com",
      }),
    });
  });

  it("会正确透传流消息发送/半关闭/关闭命令", async () => {
    invokeMock.mockResolvedValue(undefined);

    await grpcSendStreamMessage("stream-1", "{\"name\":\"jack\"}");
    await grpcEndStream("stream-1");
    await grpcCloseStream("stream-1");

    expect(invokeMock).toHaveBeenNthCalledWith(1, "grpc_send_stream_message", {
      streamId: "stream-1",
      message: "{\"name\":\"jack\"}",
    });
    expect(invokeMock).toHaveBeenNthCalledWith(2, "grpc_end_stream", {
      streamId: "stream-1",
    });
    expect(invokeMock).toHaveBeenNthCalledWith(3, "grpc_close_stream", {
      streamId: "stream-1",
    });
  });

  it("删除环境会调用 delete_environment 命令", async () => {
    invokeMock.mockResolvedValue(undefined);

    await deleteEnvironment("env-1");

    expect(invokeMock).toHaveBeenCalledWith("delete_environment", {
      id: "env-1",
    });
  });

  it("历史删除相关 API 会调用对应 tauri 命令", async () => {
    invokeMock.mockResolvedValue(undefined);

    await deleteHistory("history-1");
    await clearHistories("project-1");

    expect(invokeMock).toHaveBeenNthCalledWith(1, "delete_history_command", {
      id: "history-1",
    });
    expect(invokeMock).toHaveBeenNthCalledWith(2, "clear_histories_command", {
      projectId: "project-1",
    });
  });


  it("历史读写会透传 response_code/response_message 字段", async () => {
    invokeMock.mockReset();
    invokeMock
      .mockResolvedValueOnce(undefined)
      .mockResolvedValueOnce([
        {
          id: "history-1",
          project_id: "project-1",
          timestamp: 1710000000000,
          service: "demo.Greeter",
          method: "SayHello",
          address: "127.0.0.1:50051",
          status: "success",
          response_code: 14,
          response_message: "UNAVAILABLE",
          duration: 12,
          request_snapshot: {
            id: "req-1",
            name: "demo.Greeter/SayHello",
            type: "unary",
            service: "demo.Greeter",
            method: "SayHello",
            body: "{}",
            metadata: {},
            env_ref_type: "inherit",
          },
        },
      ]);

    await addHistory({
      id: "history-1",
      projectId: "project-1",
      timestamp: 1710000000000,
      service: "demo.Greeter",
      method: "SayHello",
      address: "127.0.0.1:50051",
      status: "success",
      responseCode: 14,
      responseMessage: "UNAVAILABLE",
      duration: 12,
      requestSnapshot: {
        id: "req-1",
        name: "demo.Greeter/SayHello",
        type: "unary",
        service: "demo.Greeter",
        method: "SayHello",
        body: "{}",
        metadata: {},
        envRefType: "inherit",
      },
    });

    const histories = await getHistories(10);

    expect(invokeMock).toHaveBeenNthCalledWith(1, "add_history", {
      history: expect.objectContaining({
        response_code: 14,
        response_message: "UNAVAILABLE",
      }),
    });
    expect(histories[0]).toMatchObject({
      responseCode: 14,
      responseMessage: "UNAVAILABLE",
    });
  });

});
