import { describe, expect, it } from "vitest";

import { formatDuration } from "../../src/lib/utils";

describe("formatDuration", () => {
  it("毫秒小于 1 时按微秒显示", () => {
    expect(formatDuration(0.456)).toBe("456.00μs");
  });

  it("毫秒在 1 到 1000 之间时按毫秒显示", () => {
    expect(formatDuration(12.3456)).toBe("12.35ms");
  });

  it("毫秒大于等于 1000 时按秒显示", () => {
    expect(formatDuration(2345)).toBe("2.35s");
  });
});
