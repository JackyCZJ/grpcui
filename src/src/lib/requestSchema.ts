import type { MethodInputSchema, MessageFieldSchema } from "../types";

/**
 * schemaFieldDefaultValue 根据字段 schema 生成默认 JSON 值。
 *
 * 该函数用于“字段化请求体”初始化与增量补齐：
 * - scalar 给出稳定零值；
 * - enum 默认首个枚举值；
 * - message 递归生成子对象；
 * - map/repeated 默认空容器。
 */
export function schemaFieldDefaultValue(field: MessageFieldSchema): unknown {
  if (field.map) {
    return {};
  }

  if (field.repeated) {
    return [];
  }

  switch (field.kind) {
    case "message": {
      const nested: Record<string, unknown> = {};
      for (const nestedField of field.fields) {
        nested[nestedField.jsonName] = schemaFieldDefaultValue(nestedField);
      }
      return nested;
    }
    case "enum":
      return field.enumValues[0] ?? "";
    case "scalar":
    default:
      switch (field.type) {
        case "string":
          return "";
        case "bool":
          return false;
        case "bytes":
          return "";
        case "double":
        case "float":
        case "int32":
        case "sint32":
        case "sfixed32":
        case "uint32":
        case "fixed32":
        case "int64":
        case "sint64":
        case "sfixed64":
        case "uint64":
        case "fixed64":
          return 0;
        default:
          return null;
      }
  }
}

/**
 * buildDefaultBodyFromSchema 依据方法入参 schema 生成默认请求体对象。
 *
 * 前端首次切换方法时会把该对象序列化到编辑器，
 * 使用户看到“可直接填写”的结构而不是空 JSON。
 */
export function buildDefaultBodyFromSchema(
  schema: MethodInputSchema | null | undefined
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  if (!schema) {
    return result;
  }

  for (const field of schema.fields) {
    result[field.jsonName] = schemaFieldDefaultValue(field);
  }

  return result;
}

/**
 * getCurrentBodyObject 尝试把编辑器 JSON 文本解析为对象。
 *
 * 解析失败时返回 null，调用方据此决定降级策略。
 */
export function getCurrentBodyObject(body: string): Record<string, unknown> | null {
  try {
    const parsed = JSON.parse(body);
    if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) {
      return parsed as Record<string, unknown>;
    }
  } catch {
    // ignore
  }
  return null;
}

/**
 * mergeSchemaBodyDefaults 将“schema 默认对象”和“用户请求对象”做深度合并。
 *
 * 合并策略：
 * - 默认值与请求值同为对象：递归合并，保留默认结构并覆盖已填写字段
 * - 默认值为数组：仅在请求值也是数组时覆盖
 * - 其他标量：请求值存在时覆盖，否则保持默认值
 */
function mergeSchemaBodyDefaults(defaultValue: unknown, requestValue: unknown): unknown {
  if (Array.isArray(defaultValue)) {
    return Array.isArray(requestValue) ? requestValue : defaultValue;
  }

  if (defaultValue && typeof defaultValue === 'object' && !Array.isArray(defaultValue)) {
    const merged = structuredClone(defaultValue) as Record<string, unknown>;

    if (!requestValue || typeof requestValue !== 'object' || Array.isArray(requestValue)) {
      return merged;
    }

    const requestObject = requestValue as Record<string, unknown>;
    for (const [key, value] of Object.entries(requestObject)) {
      if (Object.prototype.hasOwnProperty.call(merged, key)) {
        merged[key] = mergeSchemaBodyDefaults(merged[key], value);
      } else {
        merged[key] = value;
      }
    }

    return merged;
  }

  return requestValue === undefined ? defaultValue : requestValue;
}

/**
 * buildSchemaBodyFromRequest 基于 schema 默认值和当前请求体构建“可渲染表单对象”。
 *
 * 该函数可确保：
 * - 已填写字段沿用用户请求值
 * - 未填写字段仍展示 schema 默认值
 * - 非法 JSON 自动降级为纯默认对象
 */
export function buildSchemaBodyFromRequest(
  schema: MethodInputSchema | null | undefined,
  body: string
): Record<string, unknown> {
  const schemaDefaults = buildDefaultBodyFromSchema(schema);
  const currentBody = getCurrentBodyObject(body);

  if (!currentBody) {
    return schemaDefaults;
  }

  return mergeSchemaBodyDefaults(schemaDefaults, currentBody) as Record<string, unknown>;
}

/**
 * replacePathValue 在 JSON 对象里按路径替换字段值，返回新对象。
 *
 * 采用不可变更新，避免直接修改旧对象导致 React 状态不可预测。
 */
export function replacePathValue(
  source: Record<string, unknown>,
  path: string[],
  value: unknown
): Record<string, unknown> {
  const clone = structuredClone(source) as Record<string, unknown>;
  let current: Record<string, unknown> = clone;

  for (let index = 0; index < path.length - 1; index += 1) {
    const key = path[index];
    const next = current[key];

    if (!next || typeof next !== "object" || Array.isArray(next)) {
      current[key] = {};
    }

    current = current[key] as Record<string, unknown>;
  }

  current[path[path.length - 1]] = value;
  return clone;
}

/**
 * coerceInputValue 将输入框字符串转换为目标字段类型值。
 */
export function coerceInputValue(raw: string, field: MessageFieldSchema): unknown {
  if (field.kind === "enum") {
    return raw;
  }

  if (field.kind !== "scalar") {
    return raw;
  }

  switch (field.type) {
    case "bool":
      return raw === "true";
    case "double":
    case "float":
    case "int32":
    case "sint32":
    case "sfixed32":
    case "uint32":
    case "fixed32":
    case "int64":
    case "sint64":
    case "sfixed64":
    case "uint64":
    case "fixed64": {
      const parsed = Number(raw);
      return Number.isFinite(parsed) ? parsed : 0;
    }
    default:
      return raw;
  }
}
