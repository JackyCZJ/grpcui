import { invoke } from '@tauri-apps/api/core';
import type {
  Service,
  History,
  Collection,
  Environment,
  TLSConfig,
  MethodInputSchema,
  RequestItem as UiRequestItem,
} from '../types';
import type {
  Project,
  ProjectEnvironment,
  ProjectCollection,
  RequestItem as ProjectRequestItem,
  EnvRefType,
} from '../types/project';

interface ConnectRequest {
  address: string;
  tls?: TLSConfig;
  insecure?: boolean;
  proto_file?: string;
  proto_files?: string[];
  import_paths?: string[];
  use_reflection?: boolean;
}

interface ConnectResponse {
  success: boolean;
  error?: string;
}

interface DisconnectResponse {
  success: boolean;
  error?: string;
}

interface ListServicesResponse {
  services: Service[];
}

interface InvokeRequest {
  method: string;
  body: string;
  metadata?: Record<string, string>;
  address?: string;
  authority?: string;
  tls?: TLSConfig;
}

interface InvokeResponse {
  data?: unknown;
  error?: string;
  metadata: Record<string, string>;
  duration: number;
  status: string;
  code: number;
  message: string;
}

interface StreamInvokeRequest {
  method: string;
  body: string;
  metadata?: Record<string, string>;
  streamType?: 'client' | 'server' | 'bidi';
  address?: string;
  authority?: string;
  tls?: TLSConfig;
}

interface RawProject {
  id: string;
  name: string;
  description: string;
  default_environment_id?: string;
  proto_files?: string[];
  created_at?: string;
  updated_at?: string;
}

interface RawEnvironment {
  id: string;
  project_id?: string;
  name: string;
  base_url: string;
  variables?: Record<string, string>;
  headers?: Record<string, string>;
  tls_config?: {
    enabled?: boolean;
    ca_file?: string;
    cert_file?: string;
    key_file?: string;
    server_name?: string;
    insecure?: boolean;
  } | null;
  is_default?: boolean;
  created_at?: string;
  updated_at?: string;
}

interface RawRequestItem {
  id: string;
  name: string;
  type: UiRequestItem['type'];
  service: string;
  method: string;
  body: string;
  metadata?: Record<string, string>;
  env_ref_type?: EnvRefType;
  environment_id?: string;
}

interface RawFolder {
  id: string;
  name: string;
  items?: RawRequestItem[];
}

interface RawCollection {
  id: string;
  project_id?: string;
  name: string;
  folders?: RawFolder[];
  items?: RawRequestItem[];
  created_at?: string;
  updated_at?: string;
}

interface RawHistory {
  id: string;
  project_id?: string;
  timestamp: number;
  service: string;
  method: string;
  address: string;
  status: 'success' | 'error';
  response_code?: number;
  response_message?: string;
  duration: number;
  request_snapshot: RawRequestItem;
}

interface GrpcConnectOptions {
  protoFile?: string;
  protoFiles?: string[];
  importPaths?: string[];
  useReflection?: boolean;
}

interface ProtoFileDiscoveryResponse {
  rootDir: string;
  absolutePaths: string[];
  relativePaths: string[];
}

interface GetMethodInputSchemaRequest {
  service: string;
  method: string;
}

const DEFAULT_GRPC_ADDRESS = 'localhost:50051';

// resolveInvokeTransport 统一解析调用请求的地址、authority 与 metadata。
//
// 兼容旧逻辑：
// - 若显式传入 `address`，优先使用它；
// - 否则回退默认地址，避免触发后端 `missing field address`。
//
// 设计上 authority 必须走独立字段，不再从 metadata 里做兼容提取，
// 以明确区分“连接层属性”与“业务 metadata”。
// 同时会丢弃所有伪首部（以 `:` 开头），避免把 HTTP/2 控制字段当业务头透传。
function resolveInvokeTransport(request: {
  address?: string;
  metadata?: Record<string, string>;
  authority?: string;
}): { address: string; authority?: string; metadata?: Record<string, string> } {
  const metadata = request.metadata
    ? Object.fromEntries(
        Object.entries(request.metadata).filter(([key]) => !key.trim().startsWith(':'))
      )
    : undefined;
  const explicitAuthority = request.authority?.trim();
  const explicitAddress = request.address?.trim();
  const resolvedAddress = explicitAddress || DEFAULT_GRPC_ADDRESS;

  return {
    address: resolvedAddress,
    authority: explicitAuthority || undefined,
    metadata: metadata && Object.keys(metadata).length > 0 ? metadata : undefined,
  };
}
// mapTlsConfigToRaw 将前端 TLS 配置转换为后端 ConnectRequest 所需结构。
//
// Rust 侧 `ConnectRequest.tls` 反序列化目标是存储层 TLSConfig，字段要求为：
// - enabled
// - ca_file / cert_file / key_file
// - insecure
//
// 若这里继续发送旧字段（mode/ca_cert/client_cert/client_key），
// 会触发 `missing field \`enabled\``，导致导入 proto / reflection 连接直接失败。
function mapTlsConfigToRaw(tls?: TLSConfig): RawEnvironment["tls_config"] | undefined {
  if (!tls) {
    return undefined;
  }

  return mapRawTlsConfigToStore(tls);
}

// mapRawTlsConfigToUi 将 sidecar 的 TLS 结构转换为前端展示结构，
// 保证 UI 始终消费统一的 mode/caCert/clientCert/clientKey 语义。
function mapRawTlsConfigToUi(raw?: RawEnvironment['tls_config']): TLSConfig {
  if (!raw || raw.enabled === false) {
    return {
      mode: 'insecure',
      authority: raw?.server_name,
    };
  }

  if (raw.ca_file || raw.cert_file || raw.key_file) {
    return {
      mode: 'custom',
      authority: raw.server_name,
      caCert: raw.ca_file,
      clientCert: raw.cert_file,
      clientKey: raw.key_file,
      skipVerify: raw.insecure,
    };
  }

  return {
    mode: 'system',
    authority: raw.server_name,
    skipVerify: raw.insecure,
  };
}

// mapVariablesToRecord 将变量数组压平为键值对，
// 便于 sidecar 在存储层直接按 map 落库。
function mapVariablesToRecord(variables: Environment['variables']): Record<string, string> {
  return variables.reduce<Record<string, string>>((acc, variable) => {
    acc[variable.key] = variable.value;
    return acc;
  }, {});
}

// mapRecordToVariables 将存储层 map 还原为变量数组，
// 这样前端可以保持既有变量编辑交互模型。
function mapRecordToVariables(values?: Record<string, string>) {
  return Object.entries(values ?? {}).map(([key, value]) => ({
    key,
    value,
    secret: false,
  }));
}

// mapRawProjectToUi 负责项目字段 snake_case -> camelCase 转换。
function mapRawProjectToUi(raw: RawProject): Project {
  return {
    id: raw.id,
    name: raw.name,
    description: raw.description,
    defaultEnvironmentId: raw.default_environment_id,
    protoFiles: raw.proto_files,
    createdAt: raw.created_at ?? new Date().toISOString(),
    updatedAt: raw.updated_at ?? new Date().toISOString(),
  };
}

// mapProjectToRaw 负责项目字段 camelCase -> snake_case 转换。
function mapProjectToRaw(project: Project): RawProject {
  const now = new Date().toISOString();
  return {
    id: project.id,
    name: project.name,
    description: project.description,
    default_environment_id: project.defaultEnvironmentId,
    proto_files: project.protoFiles,
    created_at: project.createdAt || now,
    updated_at: project.updatedAt || now,
  };
}

// mapRawRequestItemToUi 统一解析请求项环境引用策略，
// 在缺省场景下默认走 inherit，提升历史数据兼容性。
function mapRawRequestItemToUi(raw: RawRequestItem): ProjectRequestItem {
  return {
    id: raw.id,
    name: raw.name,
    type: raw.type,
    service: raw.service,
    method: raw.method,
    body: raw.body,
    metadata: raw.metadata ?? {},
    envRefType: raw.env_ref_type ?? 'inherit',
    environmentId: raw.environment_id,
  };
}

// mapRequestItemToRaw 将前端请求项转换为后端字段，
// 确保 envRefType/environmentId 在保存收藏与历史时不丢失。
function mapRequestItemToRaw(item: UiRequestItem | ProjectRequestItem): RawRequestItem {
  return {
    id: item.id,
    name: item.name,
    type: item.type,
    service: item.service,
    method: item.method,
    body: item.body,
    metadata: item.metadata,
    env_ref_type: item.envRefType ?? 'inherit',
    environment_id: item.environmentId,
  };
}

// mapRawEnvironmentToUi 将环境实体转换为前端统一结构，
// 同时附带项目维度和默认环境状态，支持项目-环境联动。
function mapRawEnvironmentToUi(raw: RawEnvironment): ProjectEnvironment {
  return {
    id: raw.id,
    projectId: raw.project_id ?? '',
    name: raw.name,
    baseUrl: raw.base_url,
    tls: mapRawTlsConfigToUi(raw.tls_config),
    metadata: raw.headers ?? {},
    variables: mapRecordToVariables(raw.variables),
    isDefault: Boolean(raw.is_default),
    createdAt: raw.created_at ?? new Date().toISOString(),
    updatedAt: raw.updated_at ?? new Date().toISOString(),
  };
}

// mapEnvironmentToRaw 将前端环境模型转换为 sidecar 存储结构。
function mapEnvironmentToRaw(environment: Environment | ProjectEnvironment): RawEnvironment {
  const now = new Date().toISOString();
  return {
    id: environment.id,
    project_id: environment.projectId,
    name: environment.name,
    base_url: environment.baseUrl,
    variables: mapVariablesToRecord(environment.variables),
    headers: environment.metadata,
    tls_config: mapRawTlsConfigToStore(environment.tls),
    is_default: Boolean(environment.isDefault),
    created_at: (environment as ProjectEnvironment).createdAt || now,
    updated_at: (environment as ProjectEnvironment).updatedAt || now,
  };
}

// mapRawTlsConfigToStore 将前端 TLS 配置转换成 sidecar 可持久化的 TLS 结构。
function mapRawTlsConfigToStore(tls: TLSConfig): RawEnvironment['tls_config'] {
  if (tls.mode === 'insecure') {
    return {
      enabled: false,
      server_name: tls.authority,
      insecure: true,
    };
  }

  return {
    enabled: true,
    server_name: tls.authority,
    ca_file: tls.caCert,
    cert_file: tls.clientCert,
    key_file: tls.clientKey,
    insecure: tls.skipVerify,
  };
}

// mapRawCollectionToUi 将收藏数据转换为项目维度结构。
function mapRawCollectionToUi(raw: RawCollection): ProjectCollection {
  return {
    id: raw.id,
    projectId: raw.project_id ?? '',
    name: raw.name,
    folders: (raw.folders ?? []).map((folder) => ({
      id: folder.id,
      name: folder.name,
      items: (folder.items ?? []).map(mapRawRequestItemToUi),
    })),
    items: (raw.items ?? []).map(mapRawRequestItemToUi),
    createdAt: raw.created_at ?? new Date().toISOString(),
    updatedAt: raw.updated_at ?? new Date().toISOString(),
  };
}

// mapCollectionToRaw 将收藏模型转换为后端存储格式。
function mapCollectionToRaw(collection: Collection | ProjectCollection): RawCollection {
  return {
    id: collection.id,
    project_id: collection.projectId,
    name: collection.name,
    folders: (collection.folders ?? []).map((folder) => ({
      id: folder.id,
      name: folder.name,
      items: (folder.items ?? []).map(mapRequestItemToRaw),
    })),
    items: (collection.items ?? []).map(mapRequestItemToRaw),
    created_at: (collection as ProjectCollection).createdAt,
    updated_at: (collection as ProjectCollection).updatedAt,
  };
}

// mapRawHistoryToUi 将历史记录转换为前端结构，
// 并补齐 requestSnapshot 的 envRefType 兼容字段。
function mapRawHistoryToUi(raw: RawHistory): History {
  return {
    id: raw.id,
    projectId: raw.project_id,
    timestamp: raw.timestamp,
    service: raw.service,
    method: raw.method,
    address: raw.address,
    status: raw.status,
    responseCode: raw.response_code,
    responseMessage: raw.response_message,
    duration: raw.duration,
    requestSnapshot: {
      ...mapRawRequestItemToUi(raw.request_snapshot),
    },
  };
}

// mapHistoryToRaw 将前端历史记录转换为 sidecar 存储格式。
function mapHistoryToRaw(history: History): RawHistory {
  return {
    id: history.id,
    project_id: history.projectId,
    timestamp: history.timestamp,
    service: history.service,
    method: history.method,
    address: history.address,
    status: history.status,
    response_code: history.responseCode,
    response_message: history.responseMessage,
    duration: history.duration,
    request_snapshot: mapRequestItemToRaw(history.requestSnapshot),
  };
}

/**
 * Connect to a gRPC server
 */
export async function grpcConnect(
  address: string,
  tls?: TLSConfig,
  options?: GrpcConnectOptions
): Promise<ConnectResponse> {
  return invoke<ConnectResponse>('grpc_connect', {
    request: {
      address,
      tls: mapTlsConfigToRaw(tls),
      insecure: tls?.mode === 'insecure',
      proto_file: options?.protoFile,
      proto_files: options?.protoFiles,
      import_paths: options?.importPaths,
      use_reflection: options?.useReflection ?? true,
    } as ConnectRequest,
  });
}

/**
 * Disconnect from gRPC server
 */
export async function grpcDisconnect(): Promise<DisconnectResponse> {
  return invoke<DisconnectResponse>('grpc_disconnect');
}

/**
 * discoverProtoFiles 负责调用后端扫描目录下的 proto 文件。
 *
 * 目录扫描放在 Rust 侧执行，可避免前端 fs 权限受限导致“导入目录无反应”。
 */
export async function discoverProtoFiles(rootDir: string): Promise<ProtoFileDiscoveryResponse> {
  return invoke<ProtoFileDiscoveryResponse>('discover_proto_files', {
    request: {
      root_dir: rootDir,
    },
  });
}

/**
 * List available gRPC services
 */
export async function grpcListServices(): Promise<ListServicesResponse> {
  return invoke<ListServicesResponse>('grpc_list_services');
}

/**
 * Get method input schema for field-based request body editing
 */
export async function grpcGetMethodInputSchema(
  service: string,
  method: string
): Promise<MethodInputSchema> {
  return invoke<MethodInputSchema>('grpc_get_method_input_schema', {
    request: {
      service,
      method,
    } as GetMethodInputSchemaRequest,
  });
}

/**
 * Invoke a unary gRPC method
 */
export async function grpcInvoke(request: InvokeRequest): Promise<InvokeResponse> {
  const transport = resolveInvokeTransport(request);

  return invoke<InvokeResponse>('grpc_invoke', {
    request: {
      method: request.method,
      body: request.body,
      metadata: transport.metadata,
      address: transport.address,
      authority: transport.authority,
      tls: mapTlsConfigToRaw(request.tls),
    },
  });
}

/**
 * Start a streaming gRPC call
 * Returns a stream ID for event-based streaming
 */
export async function grpcInvokeStream(request: StreamInvokeRequest): Promise<string> {
  const transport = resolveInvokeTransport(request);

  return invoke<string>('grpc_invoke_stream', {
    request: {
      method: request.method,
      body: request.body,
      metadata: transport.metadata,
      address: transport.address,
      authority: transport.authority,
      tls: mapTlsConfigToRaw(request.tls),
      stream_type: request.streamType ?? 'server',
    },
  });
}

/**
 * Send a message to an active client/bidi stream
 */
export async function grpcSendStreamMessage(streamId: string, message: string): Promise<void> {
  return invoke<void>('grpc_send_stream_message', { streamId, message });
}

/**
 * End an active stream input (half-close for client/bidi streams)
 */
export async function grpcEndStream(streamId: string): Promise<void> {
  return invoke<void>('grpc_end_stream', { streamId });
}


/**
 * Close an active stream
 */
export async function grpcCloseStream(streamId: string): Promise<void> {
  return invoke<void>('grpc_close_stream', { streamId });
}

/**
 * Save an environment
 */
export async function saveEnvironment(env: Environment | ProjectEnvironment): Promise<void> {
  return invoke<void>('save_environment', { env: mapEnvironmentToRaw(env) });
}

/**
 * Delete an environment
 */
export async function deleteEnvironment(id: string): Promise<void> {
  return invoke<void>('delete_environment', { id });
}

/**
 * Get all environments
 */
export async function getEnvironments(): Promise<Environment[]> {
  const raw = await invoke<RawEnvironment[]>('get_environments');
  return raw.map(mapRawEnvironmentToUi);
}

/**
 * Get project scoped environments
 */
export async function getEnvironmentsByProject(projectId: string): Promise<ProjectEnvironment[]> {
  const raw = await invoke<RawEnvironment[]>('get_environments_by_project', {
    projectId,
  });
  return raw.map(mapRawEnvironmentToUi);
}

/**
 * Save a collection
 */
export async function saveCollection(collection: Collection | ProjectCollection): Promise<void> {
  return invoke<void>('save_collection', { collection: mapCollectionToRaw(collection) });
}

/**
 * Get all collections
 */
export async function getCollections(): Promise<Collection[]> {
  const raw = await invoke<RawCollection[]>('get_collections');
  return raw.map(mapRawCollectionToUi);
}

/**
 * Get project scoped collections
 */
export async function getCollectionsByProject(projectId: string): Promise<ProjectCollection[]> {
  const raw = await invoke<RawCollection[]>('get_collections_by_project', {
    projectId,
  });
  return raw.map(mapRawCollectionToUi);
}

/**
 * List projects
 */
export async function getProjects(): Promise<Project[]> {
  const raw = await invoke<RawProject[]>('get_projects');
  return raw.map(mapRawProjectToUi);
}

/**
 * Create project
 */
export async function createProject(project: Project): Promise<Project> {
  const raw = await invoke<RawProject>('create_project', {
    project: mapProjectToRaw(project),
  });
  return mapRawProjectToUi(raw);
}

/**
 * Update project
 */
export async function updateProject(project: Project): Promise<void> {
  return invoke<void>('update_project', {
    project: mapProjectToRaw(project),
  });
}

/**
 * Delete project
 */
export async function deleteProject(id: string): Promise<void> {
  return invoke<void>('delete_project', { id });
}

/**
 * Clone project
 */
export async function cloneProject(id: string, newName: string): Promise<Project> {
  const raw = await invoke<RawProject>('clone_project', {
    id,
    newName,
  });
  return mapRawProjectToUi(raw);
}

/**
 * Set default environment for a project
 */
export async function setDefaultEnvironment(projectId: string, envId: string): Promise<void> {
  return invoke<void>('set_default_environment', {
    projectId,
    envId,
  });
}

/**
 * Add a history entry
 */
export async function addHistory(history: History): Promise<void> {
  return invoke<void>('add_history', {
    history: mapHistoryToRaw(history),
  });
}

/**
 * Get history entries
 */
export async function getHistories(limit?: number): Promise<History[]> {
  const raw = await invoke<RawHistory[]>('get_histories', { limit });
  return raw.map(mapRawHistoryToUi);
}

// deleteHistory 按历史记录 ID 删除单条调用记录。
//
// 该函数用于历史列表的“删除”按钮，删除成功后前端会同步刷新/裁剪本地列表，
// 以保证界面状态与数据库保持一致。
export async function deleteHistory(id: string): Promise<void> {
  return invoke<void>('delete_history_command', { id });
}

// clearHistories 按 projectId 清空当前项目的调用历史。
//
// 该设计用于历史页“删除全部（当前项目）”入口，
// 可确保只影响当前项目的数据，不会误删其他项目历史。
export async function clearHistories(projectId: string): Promise<void> {
  return invoke<void>('clear_histories_command', { projectId });
}

export const tauriApi = {
  grpcConnect,
  grpcDisconnect,
  discoverProtoFiles,
  grpcListServices,
  grpcGetMethodInputSchema,
  grpcInvoke,
  grpcInvokeStream,
  grpcSendStreamMessage,
  grpcEndStream,
  grpcCloseStream,
  saveEnvironment,
  deleteEnvironment,
  getEnvironments,
  getEnvironmentsByProject,
  saveCollection,
  getCollections,
  getCollectionsByProject,
  getProjects,
  createProject,
  updateProject,
  deleteProject,
  cloneProject,
  setDefaultEnvironment,
  addHistory,
  getHistories,
  deleteHistory,
  clearHistories,
};

export default tauriApi;
