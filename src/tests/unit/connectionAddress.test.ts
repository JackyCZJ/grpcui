import { describe, expect, it } from "vitest";

import {
  normalizeProtoDialogSelection,
  resolveAddressForEnvironmentSelection,
  resolveConnectionAddress,
} from "../../src/lib/connectionAddress";

describe("resolveConnectionAddress", () => {
  it("优先使用用户输入地址", () => {
    expect(resolveConnectionAddress(" 127.0.0.1:50051 ", "env:8443")).toBe(
      "127.0.0.1:50051"
    );
  });

  it("输入地址为空时回退到环境地址", () => {
    expect(resolveConnectionAddress("", "grpc.example.com:443")).toBe(
      "grpc.example.com:443"
    );
  });

  it("输入地址和环境地址都为空时回退默认地址", () => {
    expect(resolveConnectionAddress("", "")).toBe("localhost:50051");
  });
});

describe("resolveAddressForEnvironmentSelection", () => {
  const environments = [
    { id: "env-1", baseUrl: "prod.example.com:443" },
    { id: "env-2", baseUrl: "  " },
  ];

  it("切换到存在的环境时返回环境地址", () => {
    expect(resolveAddressForEnvironmentSelection(environments, "env-1")).toBe(
      "prod.example.com:443"
    );
  });

  it("环境地址为空时返回 null", () => {
    expect(resolveAddressForEnvironmentSelection(environments, "env-2")).toBeNull();
  });

  it("未匹配到环境时返回 null", () => {
    expect(resolveAddressForEnvironmentSelection(environments, "env-missing")).toBeNull();
  });
});

describe("normalizeProtoDialogSelection", () => {
  it("字符串路径保持原样返回", () => {
    expect(normalizeProtoDialogSelection("/tmp/demo.proto")).toBe(
      "/tmp/demo.proto"
    );
  });

  it("数组路径取第一个元素", () => {
    expect(
      normalizeProtoDialogSelection(["/tmp/a.proto", "/tmp/b.proto"])
    ).toBe("/tmp/a.proto");
  });

  it("空值或空数组返回 null", () => {
    expect(normalizeProtoDialogSelection(null)).toBeNull();
    expect(normalizeProtoDialogSelection([])).toBeNull();
  });
});
