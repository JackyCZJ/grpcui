import { describe, expect, it } from "vitest";

import type { MethodInputSchema } from "../../src/types";
import {
  buildDefaultBodyFromSchema,
  buildSchemaBodyFromRequest,
  coerceInputValue,
  replacePathValue,
} from "../../src/lib/requestSchema";

const sampleSchema: MethodInputSchema = {
  typeName: "demo.CreateUserRequest",
  fields: [
    {
      name: "name",
      jsonName: "name",
      kind: "scalar",
      type: "string",
      repeated: false,
      required: false,
      optional: false,
      enumValues: [],
      fields: [],
      map: false,
      mapValueEnumValues: [],
      mapValueFields: [],
    },
    {
      name: "enabled",
      jsonName: "enabled",
      kind: "scalar",
      type: "bool",
      repeated: false,
      required: false,
      optional: false,
      enumValues: [],
      fields: [],
      map: false,
      mapValueEnumValues: [],
      mapValueFields: [],
    },
    {
      name: "status",
      jsonName: "status",
      kind: "enum",
      type: "demo.Status",
      repeated: false,
      required: false,
      optional: false,
      enumValues: ["STATUS_UNSPECIFIED", "STATUS_ACTIVE"],
      fields: [],
      map: false,
      mapValueEnumValues: [],
      mapValueFields: [],
    },
    {
      name: "profile",
      jsonName: "profile",
      kind: "message",
      type: "demo.Profile",
      repeated: false,
      required: false,
      optional: false,
      enumValues: [],
      fields: [
        {
          name: "email",
          jsonName: "email",
          kind: "scalar",
          type: "string",
          repeated: false,
          required: false,
          optional: false,
          enumValues: [],
          fields: [],
          map: false,
          mapValueEnumValues: [],
          mapValueFields: [],
        },
      ],
      map: false,
      mapValueEnumValues: [],
      mapValueFields: [],
    },
    {
      name: "tags",
      jsonName: "tags",
      kind: "scalar",
      type: "string",
      repeated: true,
      required: false,
      optional: false,
      enumValues: [],
      fields: [],
      map: false,
      mapValueEnumValues: [],
      mapValueFields: [],
    },
  ],
};

describe("buildDefaultBodyFromSchema", () => {
  it("生成可编辑的默认请求体", () => {
    expect(buildDefaultBodyFromSchema(sampleSchema)).toEqual({
      name: "",
      enabled: false,
      status: "STATUS_UNSPECIFIED",
      profile: {
        email: "",
      },
      tags: [],
    });
  });
});

describe("replacePathValue", () => {
  it("支持按路径不可变更新嵌套字段", () => {
    const source = {
      profile: {
        email: "",
      },
      status: "STATUS_UNSPECIFIED",
    };

    const next = replacePathValue(source, ["profile", "email"], "demo@example.com");

    expect(next).toEqual({
      profile: {
        email: "demo@example.com",
      },
      status: "STATUS_UNSPECIFIED",
    });
    expect(source.profile.email).toBe("");
  });
});

describe("coerceInputValue", () => {
  it("按字段类型把字符串转换为目标值", () => {
    const stringField = sampleSchema.fields.find((field) => field.jsonName === "name");
    if (!stringField) {
      throw new Error("schema missing name field");
    }

    const boolField = sampleSchema.fields.find((field) => field.jsonName === "enabled");
    if (!boolField) {
      throw new Error("schema missing enabled field");
    }

    expect(coerceInputValue("hello", stringField)).toBe("hello");
    expect(coerceInputValue("true", boolField)).toBe(true);
  });
});

describe("buildSchemaBodyFromRequest", () => {
  it("合并请求体与 schema 默认值，确保表单可完整渲染", () => {
    const merged = buildSchemaBodyFromRequest(
      sampleSchema,
      JSON.stringify({
        name: "alice",
        profile: {
          email: "alice@example.com",
        },
      })
    );

    expect(merged).toEqual({
      name: "alice",
      enabled: false,
      status: "STATUS_UNSPECIFIED",
      profile: {
        email: "alice@example.com",
      },
      tags: [],
    });
  });

  it("JSON 非法时回退为 schema 默认值", () => {
    const merged = buildSchemaBodyFromRequest(sampleSchema, "{ invalid json ");

    expect(merged).toEqual(buildDefaultBodyFromSchema(sampleSchema));
  });
});
