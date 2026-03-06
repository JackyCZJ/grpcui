// ============ gRPC Service Types ============

export interface Service {
  name: string;
  fullName: string;
  /**
   * sourcePath 表示服务定义所在 proto 文件路径。
   *
   * - 本地导入时通常为相对导入根目录的路径
   * - reflection 场景可能不存在该字段
   */
  sourcePath?: string;
  methods: Method[];
}

export interface Method {
  name: string;
  fullName: string;
  inputType: string;
  outputType: string;
  type: MethodType;
}

/**
 * MethodInputSchema 描述方法入参消息的可编辑结构。
 *
 * 前端可基于该结构渲染字段化表单，避免用户手写整段 JSON。
 */
export interface MethodInputSchema {
  typeName: string;
  fields: MessageFieldSchema[];
}

/**
 * MessageFieldSchema 描述单个 protobuf 字段的编辑信息。
 */
export interface MessageFieldSchema {
  name: string;
  jsonName: string;
  kind: 'scalar' | 'enum' | 'message' | 'map';
  type: string;
  repeated: boolean;
  required: boolean;
  optional: boolean;
  oneOf?: string;
  enumValues: string[];
  fields: MessageFieldSchema[];
  map: boolean;
  mapKeyType?: string;
  mapValueKind?: 'scalar' | 'enum' | 'message';
  mapValueType?: string;
  mapValueEnumValues: string[];
  mapValueFields: MessageFieldSchema[];
}

export type MethodType = "unary" | "server_stream" | "client_stream" | "bidi_stream";

export interface Message {
  name: string;
  fields: MessageField[];
}

export interface MessageField {
  name: string;
  type: string;
  repeated: boolean;
  optional: boolean;
}

// ============ Connection Types ============

export type ConnectionState = "disconnected" | "connecting" | "connected" | "error";

export interface ConnectionConfig {
  address: string;
  tls: TLSConfig;
}

// ============ Request/Response Types ============

export interface Request {
  id: string;
  service: string;
  method: string;
  address: string;
  body: string;
  metadata: MetadataEntry[];
  environmentId?: string;
}

export interface Response {
  id: string;
  requestId: string;
  status: ResponseStatus;
  statusCode?: string;
  body: string;
  metadata: Record<string, string>;
  trailers: Record<string, string>;
  duration: number;
  timestamp: number;
  error?: string;
}

/** GrpcResponse - simplified response format for gRPC calls */
export interface GrpcResponse {
  data?: unknown;
  error?: string;
  metadata: Record<string, string>;
  duration: number;
  status: string;
  code: number;
  message: string;
}

export type ResponseStatus = "pending" | "success" | "error" | "streaming";

export interface MetadataEntry {
  id: string;
  key: string;
  value: string;
  enabled: boolean;
}

// ============ Environment Types ============

export type EnvRefType = "inherit" | "specific" | "none";

export interface Environment {
  id: string;
  projectId?: string;
  name: string;
  baseUrl: string;
  tls: TLSConfig;
  metadata: Record<string, string>;
  variables: Variable[];
  isDefault?: boolean;
  createdAt?: string;
  updatedAt?: string;
}

export interface Variable {
  key: string;
  value: string;
  secret: boolean;
}

export interface TLSConfig {
  mode: "insecure" | "system" | "custom";
  authority?: string;
  caCert?: string;
  clientCert?: string;
  clientKey?: string;
  skipVerify?: boolean;
}

export interface ResolvedEnvironment {
  baseUrl: string;
  variables: Record<string, string>;
  headers: Record<string, string>;
  tls: TLSConfig;
}

// ============ Project Types ============

export interface Project {
  id: string;
  name: string;
  description: string;
  defaultEnvironmentId?: string;
  protoFiles?: string[];
  createdAt: string;
  updatedAt: string;
}

// ============ Collection Types ============

export interface Collection {
  id: string;
  projectId?: string;
  name: string;
  folders: Folder[];
  items: RequestItem[];
  createdAt?: string;
  updatedAt?: string;
}

export interface Folder {
  id: string;
  name: string;
  items: RequestItem[];
}

export interface RequestItem {
  id: string;
  name: string;
  type: MethodType;
  service: string;
  method: string;
  body: string;
  metadata: Record<string, string>;
  environmentId?: string;
  envRefType?: EnvRefType;
}

// ============ History Types ============

export interface History {
  id: string;
  projectId?: string;
  timestamp: number;
  service: string;
  method: string;
  address: string;
  status: "success" | "error";
  responseCode?: number;
  responseMessage?: string;
  duration: number;
  requestSnapshot: RequestItem;
}

/** @deprecated Use History instead */
export type HistoryEntry = History;

// ============ Stream Types ============

export interface StreamMessage {
  id: string;
  type: "message" | "error" | "end";
  payload?: unknown;
  error?: string;
  timestamp: number;
}

// ============ UI Types ============

export type TabType = "services" | "collections" | "environments" | "history";

export interface ServiceTreeItem {
  service: Service;
  expanded: boolean;
}
