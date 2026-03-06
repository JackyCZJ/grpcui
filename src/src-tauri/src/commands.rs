use tauri::{State, AppHandle};
use serde::{Deserialize, Serialize};
use crate::AppState;
use crate::ffi::ReflectionTlsConfig;
use crate::stream_manager::StreamManager;
use crate::grpc::streaming::StreamType as GrpcStreamType;
use crate::storage::{
    ProjectStore, EnvironmentStore, CollectionStore, HistoryStore,
    CreateProject, UpdateProject, CreateEnvironment, UpdateEnvironment,
    CreateCollection, UpdateCollection, CreateHistory,
    TLSConfig, RequestItem as StorageRequestItem, Folder as StorageFolder,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub type Result<T> = std::result::Result<T, String>;

// ===== gRPC Commands =====

#[derive(Debug, Serialize, Deserialize)]
pub struct ConnectRequest {
    address: String,
    tls: Option<TLSConfig>,
    #[serde(default)]
    insecure: bool,
    #[serde(default)]
    proto_file: Option<String>,
    #[serde(default)]
    proto_files: Option<Vec<String>>,
    #[serde(default)]
    import_paths: Option<Vec<String>>,
    #[serde(default)]
    use_reflection: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ConnectResponse {
    success: bool,
    error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DiscoverProtoFilesRequest {
    root_dir: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DiscoverProtoFilesResponse {
    #[serde(rename = "rootDir")]
    root_dir: String,
    #[serde(rename = "absolutePaths")]
    absolute_paths: Vec<String>,
    #[serde(rename = "relativePaths")]
    relative_paths: Vec<String>,
}

/// 连接策略规划结果。
///
/// 通过先规划再执行，`grpc_connect` 可以把“路径选择”与“FFI 调用”解耦，
/// 这样既容易单元测试，也能让错误信息聚焦在配置层面而不是运行时细节。
#[derive(Debug, Clone, PartialEq, Eq)]
enum ConnectPlan {
    /// 使用 reflection 从远端服务动态加载描述信息。
    Reflection,
    /// 使用本地 proto 文件加载描述信息。
    ProtoFiles {
        proto_paths: Vec<String>,
        import_paths: Vec<String>,
    },
}

/// FFI 返回的服务列表响应（snake_case）。
#[derive(Debug, Deserialize)]
struct FfiServiceListResponse {
    #[serde(default)]
    services: Vec<FfiService>,
}

/// FFI 返回的服务结构（snake_case）。
#[derive(Debug, Deserialize)]
struct FfiService {
    name: String,
    full_name: String,
    #[serde(default)]
    source_path: Option<String>,
    #[serde(default)]
    methods: Vec<FfiMethod>,
}

/// FFI 返回的方法结构（snake_case）。
#[derive(Debug, Deserialize)]
struct FfiMethod {
    name: String,
    full_name: String,
    input_type: String,
    output_type: String,
    #[serde(rename = "type")]
    r#type: String,
}

/// FFI 返回的方法入参 schema（snake_case）。
#[derive(Debug, Deserialize)]
struct FfiMethodInputSchema {
    type_name: String,
    #[serde(default)]
    fields: Vec<FfiMessageField>,
}

/// FFI 返回的消息字段结构（snake_case）。
#[derive(Debug, Deserialize)]
struct FfiMessageField {
    name: String,
    json_name: String,
    kind: String,
    #[serde(rename = "type")]
    r#type: String,
    #[serde(default)]
    repeated: bool,
    #[serde(default)]
    required: bool,
    #[serde(default)]
    optional: bool,
    #[serde(default)]
    one_of: Option<String>,
    #[serde(default)]
    enum_values: Vec<String>,
    #[serde(default)]
    fields: Vec<FfiMessageField>,
    #[serde(default)]
    map: bool,
    #[serde(default)]
    map_key_type: Option<String>,
    #[serde(default)]
    map_value_kind: Option<String>,
    #[serde(default)]
    map_value_type: Option<String>,
    #[serde(default)]
    map_value_enum_values: Vec<String>,
    #[serde(default)]
    map_value_fields: Vec<FfiMessageField>,
}

/// normalize_non_empty_paths 负责把路径列表清洗为“非空 + 去首尾空格”的稳定序列。
///
/// 该函数集中处理输入规整逻辑，避免在连接规划里散落重复判断，
/// 并保证前端传入 `proto_files/import_paths` 时不会因为空字符串污染配置。
fn normalize_non_empty_paths(paths: &[String]) -> Vec<String> {
    paths
        .iter()
        .map(|path| path.trim())
        .filter(|path| !path.is_empty())
        .map(ToString::to_string)
        .collect()
}

/// dedup_paths_preserve_order 对路径列表去重并保持原有顺序。
///
/// 导入路径存在“手动传入 + 自动推导”两种来源，去重时必须保持顺序稳定，
/// 才能让同一份配置在不同运行中得到一致的解析行为。
fn dedup_paths_preserve_order(paths: Vec<String>) -> Vec<String> {
    let mut deduped = Vec::new();
    for path in paths {
        if !deduped.contains(&path) {
            deduped.push(path);
        }
    }
    deduped
}

/// append_import_path_ancestors 会把目录及其祖先目录追加到 import 路径列表。
///
/// 该策略用于绝对 proto 文件场景：
/// - 直接父目录可保证“同级 import”可解析；
/// - 祖先目录可兼容“仓库前缀 import”（例如 `common/proto/*.proto`）。
///
/// 这里限制最多追加 `max_depth` 层，避免路径过深时无界增长。
fn append_import_path_ancestors(
    mut current: Option<&Path>,
    import_paths: &mut Vec<String>,
    max_depth: usize,
) {
    let mut depth = 0usize;

    while let Some(path) = current {
        if depth >= max_depth {
            break;
        }

        let normalized = path.to_string_lossy().trim().to_string();
        if !normalized.is_empty() && normalized != "." {
            import_paths.push(normalized);
        }

        current = path.parent();
        depth += 1;
    }
}

/// expand_tilde_path 负责把 `~` 开头的目录展开为用户 Home 目录。
///
/// 目录导入可能来自手动输入路径（例如 `~/Codehub/...`），若不展开会导致后续文件系统访问失败。
fn expand_tilde_path(raw_path: &str) -> PathBuf {
    let trimmed = raw_path.trim();

    if trimmed == "~" {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home);
        }
    }

    if let Some(relative) = trimmed.strip_prefix("~/").or_else(|| trimmed.strip_prefix("~\\")) {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join(relative);
        }
    }

    PathBuf::from(trimmed)
}

/// normalize_path_for_frontend 统一将路径转换为 `/` 分隔，避免前端分组与展示受平台差异影响。
fn normalize_path_for_frontend(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// resolve_proto_root_directory 解析并校验目录导入根路径。
///
/// 该函数会处理：
/// - 去首尾空格
/// - `~` 路径展开
/// - 相对路径转绝对路径
/// - canonicalize 与目录合法性校验
fn resolve_proto_root_directory(raw_root_dir: &str) -> Result<PathBuf> {
    let trimmed = raw_root_dir.trim();
    if trimmed.is_empty() {
        return Err("导入目录不能为空".to_string());
    }

    let expanded_root = expand_tilde_path(trimmed);
    let absolute_root = if expanded_root.is_absolute() {
        expanded_root
    } else {
        std::env::current_dir()
            .map_err(|error| format!("获取当前目录失败: {error}"))?
            .join(expanded_root)
    };

    let canonical_root = absolute_root
        .canonicalize()
        .map_err(|error| format!("导入目录不存在或不可访问: {error}"))?;

    if !canonical_root.is_dir() {
        return Err("导入目录无效：选择的路径不是目录".to_string());
    }

    Ok(canonical_root)
}

/// collect_proto_files_recursive 递归扫描目录下全部 `.proto` 文件。
///
/// 返回值为绝对路径集合，调用方再统一转成相对路径，确保导入行为与前端分组一致。
fn collect_proto_files_recursive(dir: &Path, output: &mut Vec<PathBuf>) -> Result<()> {
    let entries = std::fs::read_dir(dir)
        .map_err(|error| format!("读取目录失败 ({}): {error}", dir.display()))?;

    for entry in entries {
        let entry = entry
            .map_err(|error| format!("读取目录项失败 ({}): {error}", dir.display()))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|error| format!("读取文件类型失败 ({}): {error}", path.display()))?;

        if file_type.is_dir() {
            collect_proto_files_recursive(&path, output)?;
            continue;
        }

        let is_proto = file_type.is_file()
            && path
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("proto"));

        if is_proto {
            output.push(path);
        }
    }

    Ok(())
}

/// discover_proto_files_from_root 执行目录扫描并返回“绝对路径 + 相对路径”双视图。
///
/// 该结果供前端完成两件事：
/// 1) 连接时传相对路径给 sidecar 解析 import；
/// 2) 展示时按目录分组，保证服务树与磁盘结构一致。
fn discover_proto_files_from_root(root_dir: &str) -> Result<DiscoverProtoFilesResponse> {
    let canonical_root = resolve_proto_root_directory(root_dir)?;

    let mut absolute_paths = Vec::new();
    collect_proto_files_recursive(&canonical_root, &mut absolute_paths)?;
    absolute_paths.sort_by(|left, right| {
        normalize_path_for_frontend(left).cmp(&normalize_path_for_frontend(right))
    });

    let normalized_root = normalize_path_for_frontend(&canonical_root);
    let absolute_paths = absolute_paths
        .iter()
        .map(|path| normalize_path_for_frontend(path))
        .collect::<Vec<_>>();

    let mut relative_paths = Vec::with_capacity(absolute_paths.len());
    for absolute_path in &absolute_paths {
        let relative = absolute_path
            .strip_prefix(&format!("{normalized_root}/"))
            .or_else(|| absolute_path.strip_prefix(&normalized_root))
            .unwrap_or(absolute_path);
        let relative = relative.trim_start_matches('/').to_string();
        if !relative.is_empty() {
            relative_paths.push(relative);
        }
    }

    Ok(DiscoverProtoFilesResponse {
        root_dir: normalized_root,
        absolute_paths,
        relative_paths,
    })
}

/// 根据连接请求构建执行计划。
///
/// 分支规则严格按需求执行：
/// - `use_reflection=true`：总是走 reflection；
/// - 否则优先使用 `proto_files`（支持目录导入后的批量文件）；
/// - 若 `proto_files` 为空再回退 `proto_file`（保持旧版单文件兼容）；
/// - 否则返回错误，避免出现“点击导入后无反应”的假成功状态。
fn build_connect_plan(request: &ConnectRequest) -> Result<ConnectPlan> {
    if request.use_reflection {
        return Ok(ConnectPlan::Reflection);
    }

    let mut proto_paths = request
        .proto_files
        .as_ref()
        .map(|paths| normalize_non_empty_paths(paths))
        .unwrap_or_default();

    if proto_paths.is_empty() {
        if let Some(proto_file) = request
            .proto_file
            .as_deref()
            .map(str::trim)
            .filter(|path| !path.is_empty())
        {
            proto_paths.push(proto_file.to_string());
        }
    }

    if proto_paths.is_empty() {
        return Err("连接失败：请启用 reflection 或选择一个 proto 文件".to_string());
    }

    let mut import_paths = request
        .import_paths
        .as_ref()
        .map(|paths| normalize_non_empty_paths(paths))
        .unwrap_or_default();

    for proto_file in &proto_paths {
        let proto_path = Path::new(proto_file);
        if proto_path.is_absolute() {
            append_import_path_ancestors(proto_path.parent(), &mut import_paths, 8);
            continue;
        }

        if let Some(parent) = proto_path.parent() {
            let parent = parent.to_string_lossy().trim().to_string();
            if !parent.is_empty() && parent != "." {
                import_paths.push(parent);
            }
        }
    }

    Ok(ConnectPlan::ProtoFiles {
        proto_paths,
        import_paths: dedup_paths_preserve_order(import_paths),
    })
}

/// 将命令层 TLS 配置转换为 reflection FFI 所需结构。
///
/// 该转换函数只做字段映射，不改变前端参数模型，确保兼容现有调用方。
fn build_reflection_tls_config(request: &ConnectRequest) -> Option<ReflectionTlsConfig> {
    let tls = request.tls.as_ref();

    if tls.is_none() && !request.insecure {
        return None;
    }

    Some(ReflectionTlsConfig {
        insecure: request.insecure || tls.is_some_and(|config| config.insecure),
        cert_path: tls.and_then(|config| config.cert_file.clone()),
        key_path: tls.and_then(|config| config.key_file.clone()),
        ca_path: tls.and_then(|config| config.ca_file.clone()),
    })
}

/// 把 FFI 的 snake_case 服务 JSON 转换为前端响应结构。
///
/// Go FFI 返回的字段是 `full_name/input_type`，而前端消费 `fullName/inputType`。
/// 此处统一做协议转换，避免前端引入额外兼容逻辑。
fn map_ffi_services_payload(payload: &str) -> Result<ListServicesResponse> {
    let parsed: FfiServiceListResponse =
        serde_json::from_str(payload).map_err(|error| format!("解析服务列表失败: {error}"))?;

    let services = parsed
        .services
        .into_iter()
        .map(|service| Service {
            name: service.name,
            full_name: service.full_name,
            source_path: service.source_path,
            methods: service
                .methods
                .into_iter()
                .map(|method| Method {
                    name: method.name,
                    full_name: method.full_name,
                    input_type: method.input_type,
                    output_type: method.output_type,
                    r#type: method.r#type,
                })
                .collect(),
        })
        .collect();

    Ok(ListServicesResponse { services })
}

/// map_ffi_message_field 负责将 FFI 字段结构递归映射为前端响应字段。
fn map_ffi_message_field(field: FfiMessageField) -> MessageField {
    MessageField {
        name: field.name,
        json_name: field.json_name,
        kind: field.kind,
        r#type: field.r#type,
        repeated: field.repeated,
        required: field.required,
        optional: field.optional,
        one_of: field.one_of,
        enum_values: field.enum_values,
        fields: field
            .fields
            .into_iter()
            .map(map_ffi_message_field)
            .collect(),
        map: field.map,
        map_key_type: field.map_key_type,
        map_value_kind: field.map_value_kind,
        map_value_type: field.map_value_type,
        map_value_enum_values: field.map_value_enum_values,
        map_value_fields: field
            .map_value_fields
            .into_iter()
            .map(map_ffi_message_field)
            .collect(),
    }
}

/// map_ffi_method_input_schema_payload 把 FFI 的 snake_case schema 转成前端结构。
fn map_ffi_method_input_schema_payload(payload: &str) -> Result<MethodInputSchemaResponse> {
    let parsed: FfiMethodInputSchema =
        serde_json::from_str(payload).map_err(|error| format!("解析方法入参 schema 失败: {error}"))?;

    Ok(MethodInputSchemaResponse {
        type_name: parsed.type_name,
        fields: parsed
            .fields
            .into_iter()
            .map(map_ffi_message_field)
            .collect(),
    })
}

#[tauri::command]
pub async fn discover_proto_files(
    request: DiscoverProtoFilesRequest,
) -> Result<DiscoverProtoFilesResponse> {
    discover_proto_files_from_root(&request.root_dir)
}

#[tauri::command]
pub async fn grpc_connect(
    state: State<'_, AppState>,
    request: ConnectRequest,
) -> Result<ConnectResponse> {
    // 先做分支规划，失败时直接返回可读错误，避免进入无意义的 FFI 调用。
    let connect_plan = match build_connect_plan(&request) {
        Ok(plan) => plan,
        Err(error) => {
            return Ok(ConnectResponse {
                success: false,
                error: Some(error),
            });
        }
    };

    // 复用 AppState 中已有的 FFI 实例，避免引入额外状态或架构变更。
    let ffi_result = match connect_plan {
        ConnectPlan::Reflection => {
            let tls_config = build_reflection_tls_config(&request);
            state.ffi.load_reflection(&request.address, tls_config.as_ref())
        }
        ConnectPlan::ProtoFiles {
            proto_paths,
            import_paths,
        } => state.ffi.load_proto_files(&proto_paths, &import_paths),
    };

    match ffi_result {
        Ok(()) => Ok(ConnectResponse {
            success: true,
            error: None,
        }),
        Err(error) => Ok(ConnectResponse {
            success: false,
            error: Some(error.to_string()),
        }),
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DisconnectResponse {
    success: bool,
    error: Option<String>,
}

#[tauri::command]
pub async fn grpc_disconnect(
    state: State<'_, AppState>,
    stream_manager: State<'_, Arc<StreamManager>>,
) -> Result<DisconnectResponse> {
    // 断开时同步清理 native streaming 的连接资源，避免 UI 仍显示旧连接状态。
    stream_manager.clear_grpc_channel().await;
    stream_manager.clear_grpc_codec().await;

    // 显式重置 FFI parser，确保项目切换后不会混入上一次加载的服务描述符。
    match state.ffi.reset_parser() {
        Ok(()) => Ok(DisconnectResponse {
            success: true,
            error: None,
        }),
        Err(error) => Ok(DisconnectResponse {
            success: false,
            error: Some(format!("重置 gRPC 解析状态失败: {error}")),
        }),
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Service {
    name: String,
    #[serde(rename = "fullName")]
    full_name: String,
    #[serde(rename = "sourcePath", skip_serializing_if = "Option::is_none")]
    source_path: Option<String>,
    methods: Vec<Method>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Method {
    name: String,
    #[serde(rename = "fullName")]
    full_name: String,
    #[serde(rename = "inputType")]
    input_type: String,
    #[serde(rename = "outputType")]
    output_type: String,
    #[serde(rename = "type")]
    r#type: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ListServicesResponse {
    services: Vec<Service>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GetMethodInputSchemaRequest {
    service: String,
    method: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MethodInputSchemaResponse {
    #[serde(rename = "typeName")]
    type_name: String,
    fields: Vec<MessageField>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MessageField {
    name: String,
    #[serde(rename = "jsonName")]
    json_name: String,
    kind: String,
    #[serde(rename = "type")]
    r#type: String,
    repeated: bool,
    required: bool,
    optional: bool,
    #[serde(rename = "oneOf", skip_serializing_if = "Option::is_none")]
    one_of: Option<String>,
    #[serde(rename = "enumValues")]
    enum_values: Vec<String>,
    fields: Vec<MessageField>,
    map: bool,
    #[serde(rename = "mapKeyType", skip_serializing_if = "Option::is_none")]
    map_key_type: Option<String>,
    #[serde(rename = "mapValueKind", skip_serializing_if = "Option::is_none")]
    map_value_kind: Option<String>,
    #[serde(rename = "mapValueType", skip_serializing_if = "Option::is_none")]
    map_value_type: Option<String>,
    #[serde(rename = "mapValueEnumValues")]
    map_value_enum_values: Vec<String>,
    #[serde(rename = "mapValueFields")]
    map_value_fields: Vec<MessageField>,
}

#[tauri::command]
pub async fn grpc_list_services(
    state: State<'_, AppState>,
) -> Result<ListServicesResponse> {
    // 服务发现走 FFI，确保“导入 proto / reflection 连接”后能拿到真实服务树。
    let payload = state
        .ffi
        .list_services()
        .map_err(|error| format!("获取服务列表失败: {error}"))?;

    map_ffi_services_payload(&payload)
}

#[tauri::command]
pub async fn grpc_get_method_input_schema(
    state: State<'_, AppState>,
    request: GetMethodInputSchemaRequest,
) -> Result<MethodInputSchemaResponse> {
    let payload = state
        .ffi
        .get_method_input_schema(&request.service, &request.method)
        .map_err(|error| format!("获取方法入参 schema 失败: {error}"))?;

    map_ffi_method_input_schema_payload(&payload)
}

#[derive(Debug, Serialize, Deserialize)]
pub struct InvokeRequest {
    method: String,
    body: String,
    metadata: Option<std::collections::HashMap<String, String>>,
    /// Server address (e.g., "localhost:50051")
    address: String,
    /// Optional HTTP/2 authority / TLS SNI override (e.g., "api.example.com")
    #[serde(default)]
    authority: Option<String>,
    /// Optional TLS configuration
    #[serde(default)]
    tls: Option<crate::storage::TLSConfig>,
    /// Optional timeout in seconds
    #[serde(default)]
    timeout_secs: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct InvokeResponse {
    data: Option<serde_json::Value>,
    error: Option<String>,
    metadata: std::collections::HashMap<String, String>,
    duration: u64,
    status: String,
    code: i32,
    message: String,
}

#[tauri::command]
pub async fn grpc_invoke(
    state: State<'_, AppState>,
    request: InvokeRequest,
) -> Result<InvokeResponse> {
    use crate::grpc::{GrpcClient, TlsConfig as GrpcTlsConfig};
    use std::time::Duration;

    // Debug logging
    if std::env::var("GRPC_DEBUG").is_ok() {
        log::debug!(
            "gRPC invoke: method={}, address={}",
            request.method,
            request.address
        );
    }

    let authority_override =
        resolve_request_authority(request.authority.as_deref(), request.tls.as_ref());

    // Convert TLS config
    let tls_config = request.tls.map(|tls| GrpcTlsConfig {
        insecure: tls.insecure,
        ca_cert_path: tls.ca_file,
        client_cert_path: tls.cert_file,
        client_key_path: tls.key_file,
    });

    // Connect to the gRPC server
    // 复用 grpc_connect 已加载的共享 FFI 描述，避免 invoke 阶段出现 method not found。
    let client = match GrpcClient::connect_with_codec(
        &request.address,
        tls_config,
        authority_override,
        state.ffi.clone(),
    )
    .await
    {
        Ok(client) => client,
        Err(e) => {
            return Ok(InvokeResponse {
                data: None,
                error: Some(format!("Failed to connect: {}", e)),
                metadata: std::collections::HashMap::new(),
                duration: 0,
                status: "ERROR".to_string(),
                code: -1,
                message: e.to_string(),
            });
        }
    };

    // Prepare metadata
    let metadata = request.metadata.unwrap_or_default();

    // Prepare timeout
    let timeout = request.timeout_secs.map(Duration::from_secs);

    // Perform the unary call
    let result = client
        .unary_call(&request.method, &request.body, metadata, timeout)
        .await;

    match result {
        Ok(response) => {
            // Parse the JSON payload
            let data: serde_json::Value = match serde_json::from_str(&response.json_payload) {
                Ok(v) => v,
                Err(e) => {
                    return Ok(InvokeResponse {
                        data: None,
                        error: Some(format!("Failed to parse response: {}", e)),
                        metadata: response.metadata,
                        duration: response.duration_ms,
                        status: "ERROR".to_string(),
                        code: response.status.code,
                        message: format!("Failed to parse response: {}", e),
                    });
                }
            };

            Ok(InvokeResponse {
                data: Some(data),
                error: if response.status.code == 0 {
                    None
                } else {
                    Some(response.status.message.clone())
                },
                metadata: response.metadata,
                duration: response.duration_ms,
                status: response.status.status,
                code: response.status.code,
                message: response.status.message,
            })
        }
        Err(e) => {
            Ok(InvokeResponse {
                data: None,
                error: Some(e.to_string()),
                metadata: std::collections::HashMap::new(),
                duration: 0,
                status: "ERROR".to_string(),
                code: -1,
                message: e.to_string(),
            })
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StreamInvokeRequest {
    method: String,
    body: String,
    metadata: Option<std::collections::HashMap<String, String>>,
    stream_type: String,
    /// Server address (e.g., "localhost:50051")
    address: String,
    /// Optional HTTP/2 authority / TLS SNI override (e.g., "api.example.com")
    #[serde(default)]
    authority: Option<String>,
    /// Optional TLS configuration
    #[serde(default)]
    tls: Option<crate::storage::TLSConfig>,
    /// Optional timeout in seconds
    #[serde(default)]
    timeout_secs: Option<u64>,
}

/// Parse stream type string to GrpcStreamType
fn parse_stream_type(stream_type: &str) -> Result<GrpcStreamType> {
    match stream_type {
        "server" => Ok(GrpcStreamType::ServerStreaming),
        "client" => Ok(GrpcStreamType::ClientStreaming),
        "bidi" => Ok(GrpcStreamType::Bidirectional),
        _ => Err(format!("Invalid stream type: {}. Expected 'server', 'client', or 'bidi'", stream_type)),
    }
}

/// resolve_request_authority 统一解析调用阶段的 authority 覆盖值。
///
/// 优先级策略：
/// 1) 先使用请求级 `authority`（来自前端 `:authority` 或显式字段）；
/// 2) 若未设置，再回退到环境 TLS 的 `server_name`；
/// 3) 最终返回去空格后的非空字符串。
///
/// 这样既兼容“按请求临时覆盖”，也保留“环境级长期配置”能力，
/// 可用于网关按主机路由与证书 SNI 校验场景。
fn resolve_request_authority(
    authority: Option<&str>,
    tls: Option<&crate::storage::TLSConfig>,
) -> Option<String> {
    let normalize = |value: &str| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    };

    authority
        .and_then(normalize)
        .or_else(|| tls.and_then(|config| config.server_name.as_deref()).and_then(normalize))
}

/// build_stream_tls_config 把存储层 TLS 配置转换为 gRPC 连接层配置。
///
/// 兼容策略：
/// - `enabled = false` 时直接返回 `None`，让连接层按明文（insecure）模式处理；
/// - `enabled = true` 时仅透传当前可用字段，后续可在 transport 层逐步支持更多 TLS 细节。
fn build_stream_tls_config(tls: Option<&crate::storage::TLSConfig>) -> Option<crate::grpc::TlsConfig> {
    let tls = tls?;
    if !tls.enabled {
        return None;
    }

    Some(crate::grpc::TlsConfig {
        insecure: tls.insecure,
        ca_cert_path: tls.ca_file.clone(),
        client_cert_path: tls.cert_file.clone(),
        client_key_path: tls.key_file.clone(),
    })
}

/// should_send_initial_stream_message 判断是否应在流建立后自动发送首条消息。
///
/// 设计原因：
/// - server-stream 必须携带首条请求体；
/// - client/bidi 为了便于快速验证，保留“首条消息自动发送”的默认行为；
/// - 空字符串视为“用户不希望自动发送”。
fn should_send_initial_stream_message(stream_type: GrpcStreamType, body: &str) -> bool {
    if body.trim().is_empty() {
        return false;
    }

    matches!(
        stream_type,
        GrpcStreamType::ServerStreaming
            | GrpcStreamType::ClientStreaming
            | GrpcStreamType::Bidirectional
    )
}

#[tauri::command]
pub async fn grpc_invoke_stream(
    app: AppHandle,
    state: State<'_, AppState>,
    stream_manager: State<'_, Arc<StreamManager>>,
    request: StreamInvokeRequest,
) -> Result<String> {
    use crate::grpc::GrpcClient;

    // 解析流类型，避免把非法参数带入后续连接流程。
    let stream_type = parse_stream_type(&request.stream_type)?;
    let metadata = request.metadata.unwrap_or_default();
    let authority_override =
        resolve_request_authority(request.authority.as_deref(), request.tls.as_ref());
    let tls_config = build_stream_tls_config(request.tls.as_ref());

    // 连接目标地址并把 channel 注入 stream_manager，
    // 由 native streaming 管理器统一托管 server/client/bidi 三种模式。
    let client = GrpcClient::connect_with_codec(
        &request.address,
        tls_config,
        authority_override,
        state.ffi.clone(),
    )
    .await
    .map_err(|error| format!("Failed to connect stream transport: {}", error))?;
    stream_manager
        .set_grpc_channel(client.transport_channel())
        .await;
    // 复用 grpc_connect 加载描述时的共享 codec，避免 stream 路径出现 method not found。
    stream_manager
        .set_grpc_codec(state.ffi.clone())
        .await;

    let initial_request = if stream_type == GrpcStreamType::ServerStreaming {
        Some(request.body.clone())
    } else {
        None
    };

    let stream_id = stream_manager
        .start_native_stream(
            &request.method,
            stream_type,
            initial_request,
            metadata,
            app,
        )
        .await
        .map_err(|error| error.to_string())?;

    if should_send_initial_stream_message(stream_type, &request.body)
        && stream_type != GrpcStreamType::ServerStreaming
    {
        stream_manager
            .send_native_stream_message(&stream_id, &request.body)
            .await
            .map_err(|error| error.to_string())?;
    }

    Ok(stream_id)
}

#[tauri::command]
pub async fn grpc_send_stream_message(
    _state: State<'_, AppState>,
    stream_manager: State<'_, Arc<StreamManager>>,
    stream_id: String,
    message: String,
) -> Result<()> {
    stream_manager
        .send_native_stream_message(&stream_id, &message)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn grpc_end_stream(
    _state: State<'_, AppState>,
    stream_manager: State<'_, Arc<StreamManager>>,
    stream_id: String,
) -> Result<()> {
    stream_manager
        .end_native_stream(&stream_id)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn grpc_close_stream(
    _state: State<'_, AppState>,
    stream_manager: State<'_, Arc<StreamManager>>,
    stream_id: String,
) -> Result<()> {
    // close 优先关闭 native stream；若找不到再回退 legacy 分支，兼容历史数据。
    match stream_manager.cancel_native_stream(&stream_id).await {
        Ok(()) => Ok(()),
        Err(native_error) => {
            if stream_manager.close_stream(&stream_id).await.is_ok() {
                return Ok(());
            }

            if native_error.to_string().contains("not found") {
                return Ok(());
            }

            Err(native_error.to_string())
        }
    }
}

// ===== Storage Commands =====

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Environment {
    id: String,
    #[serde(default)]
    project_id: Option<String>,
    name: String,
    #[serde(rename = "base_url")]
    base_url: String,
    #[serde(default)]
    variables: std::collections::HashMap<String, String>,
    #[serde(default)]
    headers: std::collections::HashMap<String, String>,
    #[serde(default)]
    tls_config: Option<serde_json::Value>,
    #[serde(default)]
    is_default: bool,
    #[serde(default)]
    created_at: Option<String>,
    #[serde(default)]
    updated_at: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Project {
    id: String,
    name: String,
    description: String,
    #[serde(default)]
    default_environment_id: Option<String>,
    #[serde(default)]
    proto_files: Option<Vec<String>>,
    #[serde(default)]
    created_at: Option<String>,
    #[serde(default)]
    updated_at: Option<String>,
}

fn convert_tls_config(tls: Option<serde_json::Value>) -> Option<TLSConfig> {
    tls.and_then(|v| {
        serde_json::from_value(v).ok()
    })
}

#[tauri::command]
pub async fn save_environment(
    state: State<'_, AppState>,
    env: Environment,
) -> Result<()> {
    let store = EnvironmentStore::new(&state.db);

    // Check if environment exists
    let exists = store.get_environment(&env.id).await.map_err(|e| e.to_string())?.is_some();

    if exists {
        // Update existing
        let update = UpdateEnvironment {
            name: Some(env.name),
            base_url: Some(env.base_url),
            variables: Some(env.variables),
            headers: Some(env.headers),
            tls_config: Some(convert_tls_config(env.tls_config)),
            is_default: Some(env.is_default),
        };
        store.update_environment(&env.id, &update).await.map_err(|e| e.to_string())?;
    } else {
        // Create new
        let project_id = env.project_id.ok_or("Project ID is required")?;
        let create = CreateEnvironment {
            project_id,
            name: env.name,
            base_url: env.base_url,
            variables: env.variables,
            headers: env.headers,
            tls_config: convert_tls_config(env.tls_config),
            is_default: env.is_default,
        };
        store.create_environment(&create).await.map_err(|e| e.to_string())?;
    }

    Ok(())
}

#[tauri::command]
pub async fn delete_environment(
    state: State<'_, AppState>,
    id: String,
) -> Result<()> {
    let store = EnvironmentStore::new(&state.db);
    store.delete_environment(&id).await.map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn get_environments(
    state: State<'_, AppState>,
) -> Result<Vec<Environment>> {
    let store = EnvironmentStore::new(&state.db);
    let envs = store.list_environments().await.map_err(|e| e.to_string())?;

    Ok(envs.into_iter().map(|e| Environment {
        id: e.id,
        project_id: Some(e.project_id),
        name: e.name,
        base_url: e.base_url,
        variables: e.variables,
        headers: e.headers,
        tls_config: e.tls_config.map(|t| serde_json::to_value(t).unwrap_or_default()),
        is_default: e.is_default,
        created_at: Some(e.created_at.and_utc().timestamp_millis().to_string()),
        updated_at: Some(e.updated_at.and_utc().timestamp_millis().to_string()),
    }).collect())
}

#[tauri::command]
pub async fn get_environments_by_project(
    state: State<'_, AppState>,
    project_id: String,
) -> Result<Vec<Environment>> {
    let store = EnvironmentStore::new(&state.db);
    let envs = store.list_environments_by_project(&project_id).await.map_err(|e| e.to_string())?;

    Ok(envs.into_iter().map(|e| Environment {
        id: e.id,
        project_id: Some(e.project_id),
        name: e.name,
        base_url: e.base_url,
        variables: e.variables,
        headers: e.headers,
        tls_config: e.tls_config.map(|t| serde_json::to_value(t).unwrap_or_default()),
        is_default: e.is_default,
        created_at: Some(e.created_at.and_utc().timestamp_millis().to_string()),
        updated_at: Some(e.updated_at.and_utc().timestamp_millis().to_string()),
    }).collect())
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Collection {
    id: String,
    #[serde(default)]
    project_id: Option<String>,
    name: String,
    #[serde(default)]
    folders: Vec<Folder>,
    #[serde(default)]
    items: Vec<RequestItem>,
    #[serde(default)]
    created_at: Option<String>,
    #[serde(default)]
    updated_at: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Folder {
    id: String,
    name: String,
    #[serde(default)]
    items: Vec<RequestItem>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RequestItem {
    id: String,
    name: String,
    #[serde(rename = "type")]
    r#type: String,
    service: String,
    method: String,
    body: String,
    #[serde(default)]
    metadata: std::collections::HashMap<String, String>,
    #[serde(default)]
    env_ref_type: Option<String>,
    #[serde(default)]
    environment_id: Option<String>,
}

fn convert_request_item(item: RequestItem) -> StorageRequestItem {
    StorageRequestItem {
        id: item.id,
        name: item.name,
        item_type: item.r#type,
        service: item.service,
        method: item.method,
        body: item.body,
        metadata: item.metadata,
        env_ref_type: item.env_ref_type,
        environment_id: item.environment_id,
    }
}

fn convert_storage_request_item(item: StorageRequestItem) -> RequestItem {
    RequestItem {
        id: item.id,
        name: item.name,
        r#type: item.item_type,
        service: item.service,
        method: item.method,
        body: item.body,
        metadata: item.metadata,
        env_ref_type: item.env_ref_type,
        environment_id: item.environment_id,
    }
}

fn convert_folder(folder: Folder) -> StorageFolder {
    StorageFolder {
        id: folder.id,
        name: folder.name,
        items: folder.items.into_iter().map(convert_request_item).collect(),
    }
}

fn convert_storage_folder(folder: StorageFolder) -> Folder {
    Folder {
        id: folder.id,
        name: folder.name,
        items: folder.items.into_iter().map(convert_storage_request_item).collect(),
    }
}

#[tauri::command]
pub async fn save_collection(
    state: State<'_, AppState>,
    collection: Collection,
) -> Result<()> {
    let store = CollectionStore::new(&state.db);

    // Check if collection exists
    let exists = store.get_collection(&collection.id).await.map_err(|e| e.to_string())?.is_some();

    if exists {
        // Update existing
        let update = UpdateCollection {
            name: Some(collection.name),
            folders: Some(collection.folders.into_iter().map(convert_folder).collect()),
            items: Some(collection.items.into_iter().map(convert_request_item).collect()),
        };
        store.update_collection(&collection.id, &update).await.map_err(|e| e.to_string())?;
    } else {
        // Create new
        let project_id = collection.project_id.ok_or("Project ID is required")?;
        let create = CreateCollection {
            project_id,
            name: collection.name,
            folders: collection.folders.into_iter().map(convert_folder).collect(),
            items: collection.items.into_iter().map(convert_request_item).collect(),
        };
        store.create_collection(&create).await.map_err(|e| e.to_string())?;
    }

    Ok(())
}

#[tauri::command]
pub async fn get_collections(
    state: State<'_, AppState>,
) -> Result<Vec<Collection>> {
    let store = CollectionStore::new(&state.db);
    let collections = store.list_collections().await.map_err(|e| e.to_string())?;

    Ok(collections.into_iter().map(|c| Collection {
        id: c.id,
        project_id: Some(c.project_id),
        name: c.name,
        folders: c.folders.into_iter().map(convert_storage_folder).collect(),
        items: c.items.into_iter().map(convert_storage_request_item).collect(),
        created_at: Some(c.created_at.and_utc().timestamp_millis().to_string()),
        updated_at: Some(c.updated_at.and_utc().timestamp_millis().to_string()),
    }).collect())
}

#[tauri::command]
pub async fn get_collections_by_project(
    state: State<'_, AppState>,
    project_id: String,
) -> Result<Vec<Collection>> {
    let store = CollectionStore::new(&state.db);
    let collections = store.list_collections_by_project(&project_id).await.map_err(|e| e.to_string())?;

    Ok(collections.into_iter().map(|c| Collection {
        id: c.id,
        project_id: Some(c.project_id),
        name: c.name,
        folders: c.folders.into_iter().map(convert_storage_folder).collect(),
        items: c.items.into_iter().map(convert_storage_request_item).collect(),
        created_at: Some(c.created_at.and_utc().timestamp_millis().to_string()),
        updated_at: Some(c.updated_at.and_utc().timestamp_millis().to_string()),
    }).collect())
}

#[tauri::command]
pub async fn get_projects(
    state: State<'_, AppState>,
) -> Result<Vec<Project>> {
    let store = ProjectStore::new(&state.db);
    let projects = store.list_projects().await
        .map_err(|e| e.to_string())?;

    Ok(projects.into_iter().map(|p| Project {
        id: p.id,
        name: p.name,
        description: p.description,
        default_environment_id: p.default_environment_id,
        proto_files: Some(p.proto_files),
        created_at: Some(p.created_at.and_utc().timestamp_millis().to_string()),
        updated_at: Some(p.updated_at.and_utc().timestamp_millis().to_string()),
    }).collect())
}

#[tauri::command]
pub async fn create_project(
    state: State<'_, AppState>,
    project: Project,
) -> Result<Project> {
    let store = ProjectStore::new(&state.db);

    let create = CreateProject {
        name: project.name,
        description: project.description,
        proto_files: project.proto_files.unwrap_or_default(),
    };

    let created = store.create_project(&create).await
        .map_err(|e| e.to_string())?;

    Ok(Project {
        id: created.id,
        name: created.name,
        description: created.description,
        default_environment_id: created.default_environment_id,
        proto_files: Some(created.proto_files),
        created_at: Some(created.created_at.and_utc().timestamp_millis().to_string()),
        updated_at: Some(created.updated_at.and_utc().timestamp_millis().to_string()),
    })
}

#[tauri::command]
pub async fn update_project(
    state: State<'_, AppState>,
    project: Project,
) -> Result<()> {
    let store = ProjectStore::new(&state.db);

    let update = UpdateProject {
        name: Some(project.name),
        description: Some(project.description),
        proto_files: project.proto_files,
        default_environment_id: project.default_environment_id,
    };

    store.update_project(&project.id, &update).await
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub async fn delete_project(
    state: State<'_, AppState>,
    id: String,
) -> Result<()> {
    let store = ProjectStore::new(&state.db);
    store.delete_project(&id).await
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn clone_project(
    state: State<'_, AppState>,
    id: String,
    new_name: String,
) -> Result<Project> {
    let store = ProjectStore::new(&state.db);

    let cloned = store.clone_project(&id, &new_name).await
        .map_err(|e| e.to_string())?;

    Ok(Project {
        id: cloned.id,
        name: cloned.name,
        description: cloned.description,
        default_environment_id: cloned.default_environment_id,
        proto_files: Some(cloned.proto_files),
        created_at: Some(cloned.created_at.and_utc().timestamp_millis().to_string()),
        updated_at: Some(cloned.updated_at.and_utc().timestamp_millis().to_string()),
    })
}

#[tauri::command]
pub async fn set_default_environment(
    state: State<'_, AppState>,
    project_id: String,
    env_id: String,
) -> Result<()> {
    let store = ProjectStore::new(&state.db);
    store.set_default_environment(&project_id, &env_id).await
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[derive(Debug, Serialize, Deserialize)]
pub struct History {
    id: String,
    #[serde(default)]
    project_id: Option<String>,
    timestamp: u64,
    service: String,
    method: String,
    address: String,
    status: String,
    #[serde(default)]
    response_code: Option<i32>,
    #[serde(default)]
    response_message: Option<String>,
    duration: u64,
    request_snapshot: RequestItem,
}

#[tauri::command]
pub async fn add_history(
    state: State<'_, AppState>,
    history: History,
) -> Result<()> {
    let store = HistoryStore::new(&state.db);

    let create = CreateHistory {
        project_id: history.project_id,
        timestamp: history.timestamp as i64,
        service: history.service,
        method: history.method,
        address: history.address,
        status: history.status,
        response_code: history.response_code,
        response_message: history.response_message,
        duration: history.duration as i64,
        request_snapshot: convert_request_item(history.request_snapshot),
    };

    store.add_history(&create).await.map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn get_histories(
    state: State<'_, AppState>,
    limit: Option<usize>,
) -> Result<Vec<History>> {
    let store = HistoryStore::new(&state.db);
    let histories = store.list_histories(limit.map(|l| l as i64), Some(0)).await.map_err(|e| e.to_string())?;

    Ok(histories.into_iter().map(|h| History {
        id: h.id,
        project_id: h.project_id,
        timestamp: h.timestamp as u64,
        service: h.service,
        method: h.method,
        address: h.address,
        status: h.status,
        response_code: h.response_code,
        response_message: h.response_message,
        duration: h.duration as u64,
        request_snapshot: convert_storage_request_item(h.request_snapshot),
    }).collect())
}

/// delete_history_command 负责删除单条历史记录。
///
/// 使用独立命令可以让前端在“历史列表”里按条目精确删除，
/// 避免误清空全部历史，且便于未来扩展审计或回收策略。
#[tauri::command]
pub async fn delete_history_command(
    state: State<'_, AppState>,
    id: String,
) -> Result<()> {
    let store = HistoryStore::new(&state.db);
    store.delete_history(&id).await.map_err(|e| e.to_string())?;
    Ok(())
}

/// clear_histories_command 负责清空历史记录。
///
/// 当前实现按 project_id 维度清理 history 表，用于前端“删除当前项目全部历史”入口。
/// 该命令不会影响其他项目历史，避免跨项目误删。
#[tauri::command]
pub async fn clear_histories_command(
    state: State<'_, AppState>,
    project_id: String,
) -> Result<()> {
    let store = HistoryStore::new(&state.db);
    store
        .clear_history_by_project(&project_id)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::Mutex;
    use tempfile::TempDir;

    static HOME_ENV_LOCK: Mutex<()> = Mutex::new(());

    /// create_test_file 用于在测试目录中快速创建文件并确保父目录存在。
    fn create_test_file(path: &Path) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("应能创建测试目录结构");
        }
        fs::write(path, "syntax = \"proto3\";\n").expect("应能写入测试文件");
    }

    /// 验证连接分支优先级：当 reflection 开启时应忽略 proto_file，避免策略冲突。
    #[test]
    fn test_build_connect_plan_prefers_reflection() {
        let request = ConnectRequest {
            address: "localhost:50051".to_string(),
            tls: None,
            insecure: false,
            proto_file: Some("/tmp/example.proto".to_string()),
            proto_files: None,
            import_paths: None,
            use_reflection: true,
        };

        let plan = build_connect_plan(&request).expect("reflection 分支应可生成计划");
        assert_eq!(plan, ConnectPlan::Reflection);
    }

    /// 验证 proto 导入分支会自动推导 import 路径，满足“proto 所在目录”规则。
    #[test]
    fn test_build_connect_plan_proto_branch_with_import_dir() {
        let request = ConnectRequest {
            address: "localhost:50051".to_string(),
            tls: None,
            insecure: false,
            proto_file: Some("fixtures/echo/service.proto".to_string()),
            proto_files: None,
            import_paths: None,
            use_reflection: false,
        };

        let plan = build_connect_plan(&request).expect("proto 分支应可生成计划");
        assert_eq!(
            plan,
            ConnectPlan::ProtoFiles {
                proto_paths: vec!["fixtures/echo/service.proto".to_string()],
                import_paths: vec!["fixtures/echo".to_string()],
            }
        );
    }

    /// 验证批量 proto 文件分支会优先于单文件分支，且能合并去重 import 路径。
    #[test]
    fn test_build_connect_plan_prefers_proto_files_and_dedup_import_paths() {
        let request = ConnectRequest {
            address: "localhost:50051".to_string(),
            tls: None,
            insecure: false,
            proto_file: Some("legacy.proto".to_string()),
            proto_files: Some(vec![
                "apis/user/user.proto".to_string(),
                "apis/order/order.proto".to_string(),
            ]),
            import_paths: Some(vec![
                "apis".to_string(),
                "apis/order".to_string(),
                "apis".to_string(),
            ]),
            use_reflection: false,
        };

        let plan = build_connect_plan(&request).expect("批量 proto 分支应可生成计划");
        assert_eq!(
            plan,
            ConnectPlan::ProtoFiles {
                proto_paths: vec![
                    "apis/user/user.proto".to_string(),
                    "apis/order/order.proto".to_string(),
                ],
                import_paths: vec![
                    "apis".to_string(),
                    "apis/order".to_string(),
                    "apis/user".to_string(),
                ],
            }
        );
    }

    /// 验证绝对路径单文件导入会自动补全祖先 import 目录，
    /// 以兼容“仓库前缀 import / 跨目录 import”的复杂 proto 项目。
    #[test]
    fn test_build_connect_plan_absolute_proto_file_expands_ancestor_import_paths() {
        let temp_dir = TempDir::new().expect("应能创建临时目录");
        let proto_file = temp_dir
            .path()
            .join("workspace/common/proto/user/app/user_service.proto");

        create_test_file(&proto_file);

        let proto_file_str = proto_file.to_string_lossy().to_string();
        let request = ConnectRequest {
            address: "localhost:50051".to_string(),
            tls: None,
            insecure: false,
            proto_file: Some(proto_file_str.clone()),
            proto_files: None,
            import_paths: None,
            use_reflection: false,
        };

        let plan = build_connect_plan(&request).expect("绝对路径单文件分支应可生成计划");

        match plan {
            ConnectPlan::Reflection => panic!("不应落到 reflection 分支"),
            ConnectPlan::ProtoFiles {
                proto_paths,
                import_paths,
            } => {
                assert_eq!(proto_paths, vec![proto_file_str]);

                let proto_parent = proto_file
                    .parent()
                    .expect("proto 文件应有父目录")
                    .to_string_lossy()
                    .to_string();
                let proto_root = temp_dir
                    .path()
                    .join("workspace/common/proto")
                    .to_string_lossy()
                    .to_string();
                let workspace_root = temp_dir
                    .path()
                    .join("workspace")
                    .to_string_lossy()
                    .to_string();

                assert!(import_paths.contains(&proto_parent));
                assert!(import_paths.contains(&proto_root));
                assert!(import_paths.contains(&workspace_root));

                let unique_count = import_paths
                    .iter()
                    .cloned()
                    .collect::<std::collections::HashSet<_>>()
                    .len();
                assert_eq!(unique_count, import_paths.len());
            }
        }
    }

    /// 验证无 reflection 且无 proto_file 时会给出明确错误，防止前端出现“无反应”。
    #[test]
    fn test_build_connect_plan_requires_valid_source() {
        let request = ConnectRequest {
            address: "localhost:50051".to_string(),
            tls: None,
            insecure: false,
            proto_file: None,
            proto_files: None,
            import_paths: None,
            use_reflection: false,
        };

        let error = build_connect_plan(&request).expect_err("应返回缺少连接来源的错误");
        assert!(error.contains("reflection") || error.contains("proto"));
    }

    /// 验证目录扫描会递归收集 proto 文件，并返回稳定的相对路径顺序。
    #[test]
    fn test_discover_proto_files_from_root_collects_nested_files() {
        let temp_dir = TempDir::new().expect("应能创建临时目录");
        let root = temp_dir.path().join("proto-root");

        create_test_file(&root.join("user/service.proto"));
        create_test_file(&root.join("order/payment.PROTO"));
        create_test_file(&root.join("README.md"));

        let response = discover_proto_files_from_root(root.to_string_lossy().as_ref())
            .expect("目录扫描应成功");

        assert_eq!(response.relative_paths, vec![
            "order/payment.PROTO".to_string(),
            "user/service.proto".to_string(),
        ]);
        assert_eq!(response.absolute_paths.len(), 2);
        assert!(response.absolute_paths[0].ends_with("order/payment.PROTO"));
        assert!(response.absolute_paths[1].ends_with("user/service.proto"));
    }

    /// 验证 `~/` 前缀目录可被展开，避免用户手输路径时导入失败。
    #[test]
    fn test_resolve_proto_root_directory_supports_tilde_path() {
        let _guard = HOME_ENV_LOCK.lock().expect("应能锁定 HOME 环境变量");
        let temp_dir = TempDir::new().expect("应能创建临时目录");
        let fake_home = temp_dir.path().join("home");
        let target = fake_home.join("workspace/proto");
        fs::create_dir_all(&target).expect("应能创建目标目录");

        let old_home = std::env::var("HOME").ok();
        std::env::set_var("HOME", fake_home.to_string_lossy().as_ref());
        let resolved = resolve_proto_root_directory("~/workspace/proto")
            .expect("应能解析带 ~ 的目录");
        if let Some(previous_home) = old_home {
            std::env::set_var("HOME", previous_home);
        } else {
            std::env::remove_var("HOME");
        }

        assert_eq!(resolved, target.canonicalize().expect("目录应可 canonicalize"));
    }

    /// 验证服务列表 JSON 转换能正确从 snake_case 映射到前端需要的 camelCase 字段。
    #[test]
    fn test_map_ffi_services_payload_to_frontend_shape() {
        let payload = r#"{
            "services": [
                {
                    "name": "Greeter",
                    "full_name": "demo.Greeter",
                    "source_path": "user/greeter.proto",
                    "methods": [
                        {
                            "name": "SayHello",
                            "full_name": "demo.Greeter.SayHello",
                            "input_type": "demo.HelloRequest",
                            "output_type": "demo.HelloReply",
                            "type": "unary",
                            "client_streaming": false,
                            "server_streaming": false
                        }
                    ]
                }
            ]
        }"#;

        let response = map_ffi_services_payload(payload).expect("服务列表应解析成功");
        let serialized = serde_json::to_value(&response).expect("响应应可序列化");

        assert_eq!(serialized["services"][0]["fullName"], "demo.Greeter");
        assert_eq!(serialized["services"][0]["sourcePath"], "user/greeter.proto");
        assert_eq!(serialized["services"][0]["methods"][0]["fullName"], "demo.Greeter.SayHello");
        assert_eq!(serialized["services"][0]["methods"][0]["inputType"], "demo.HelloRequest");
        assert_eq!(serialized["services"][0]["methods"][0]["outputType"], "demo.HelloReply");
        assert_eq!(serialized["services"][0]["methods"][0]["type"], "unary");
    }

    /// 验证方法入参 schema JSON 能正确映射到前端使用的 camelCase 字段。
    #[test]
    fn test_map_ffi_method_input_schema_payload_to_frontend_shape() {
        let payload = r#"{
            "type_name": "demo.CreateUserRequest",
            "fields": [
                {
                    "name": "name",
                    "json_name": "name",
                    "kind": "scalar",
                    "type": "string",
                    "repeated": false,
                    "required": false,
                    "optional": false,
                    "enum_values": [],
                    "fields": [],
                    "map": false,
                    "map_value_enum_values": [],
                    "map_value_fields": []
                },
                {
                    "name": "status",
                    "json_name": "status",
                    "kind": "enum",
                    "type": "demo.Status",
                    "repeated": false,
                    "required": false,
                    "optional": false,
                    "enum_values": ["STATUS_UNSPECIFIED", "STATUS_ACTIVE"],
                    "fields": [],
                    "map": false,
                    "map_value_enum_values": [],
                    "map_value_fields": []
                }
            ]
        }"#;

        let response = map_ffi_method_input_schema_payload(payload).expect("schema 应解析成功");
        let serialized = serde_json::to_value(&response).expect("响应应可序列化");

        assert_eq!(serialized["typeName"], "demo.CreateUserRequest");
        assert_eq!(serialized["fields"][0]["jsonName"], "name");
        assert_eq!(serialized["fields"][0]["type"], "string");
        assert_eq!(serialized["fields"][1]["kind"], "enum");
        assert_eq!(serialized["fields"][1]["enumValues"][1], "STATUS_ACTIVE");
    }

    /// 验证 TLS 配置映射逻辑，确保命令层字段能正确传递给 reflection FFI。
    #[test]
    fn test_build_reflection_tls_config_mapping() {
        let request = ConnectRequest {
            address: "localhost:50051".to_string(),
            tls: Some(TLSConfig {
                enabled: true,
                ca_file: Some("/tmp/ca.pem".to_string()),
                cert_file: Some("/tmp/client.pem".to_string()),
                key_file: Some("/tmp/client.key".to_string()),
                server_name: None,
                insecure: false,
            }),
            insecure: true,
            proto_file: None,
            proto_files: None,
            import_paths: None,
            use_reflection: true,
        };

        let tls_config = build_reflection_tls_config(&request).expect("应构建 TLS 配置");
        assert!(tls_config.insecure);
        assert_eq!(tls_config.ca_path.as_deref(), Some("/tmp/ca.pem"));
        assert_eq!(tls_config.cert_path.as_deref(), Some("/tmp/client.pem"));
        assert_eq!(tls_config.key_path.as_deref(), Some("/tmp/client.key"));
    }


    /// 验证流式 TLS 映射：disabled 场景会回退到明文连接。
    #[test]
    fn test_build_stream_tls_config_disabled_returns_none() {
        let tls = TLSConfig {
            enabled: false,
            ca_file: Some("/tmp/ca.pem".to_string()),
            cert_file: Some("/tmp/client.pem".to_string()),
            key_file: Some("/tmp/client.key".to_string()),
            server_name: Some("example.internal".to_string()),
            insecure: true,
        };

        let mapped = build_stream_tls_config(Some(&tls));
        assert!(mapped.is_none());
    }

    /// 验证流式 TLS 映射：enabled 场景会把证书路径与 insecure 标记透传。
    #[test]
    fn test_build_stream_tls_config_enabled_maps_fields() {
        let tls = TLSConfig {
            enabled: true,
            ca_file: Some("/tmp/ca.pem".to_string()),
            cert_file: Some("/tmp/client.pem".to_string()),
            key_file: Some("/tmp/client.key".to_string()),
            server_name: None,
            insecure: false,
        };

        let mapped = build_stream_tls_config(Some(&tls)).expect("enabled 场景应生成 TLS 配置");
        assert!(!mapped.insecure);
        assert_eq!(mapped.ca_cert_path.as_deref(), Some("/tmp/ca.pem"));
        assert_eq!(mapped.client_cert_path.as_deref(), Some("/tmp/client.pem"));
        assert_eq!(mapped.client_key_path.as_deref(), Some("/tmp/client.key"));
    }

    /// 验证 authority 解析优先级：请求级覆盖值应优先于环境 TLS server_name。
    #[test]
    fn test_resolve_request_authority_prefers_request_value() {
        let tls = TLSConfig {
            enabled: true,
            ca_file: None,
            cert_file: None,
            key_file: None,
            server_name: Some("tls.example.internal".to_string()),
            insecure: false,
        };

        let resolved = resolve_request_authority(Some("api.example.com"), Some(&tls));
        assert_eq!(resolved.as_deref(), Some("api.example.com"));
    }

    /// 验证 authority 解析回退：请求未提供时应使用 TLS server_name。
    #[test]
    fn test_resolve_request_authority_falls_back_to_tls_server_name() {
        let tls = TLSConfig {
            enabled: true,
            ca_file: None,
            cert_file: None,
            key_file: None,
            server_name: Some("gateway.example.com".to_string()),
            insecure: false,
        };

        let resolved = resolve_request_authority(None, Some(&tls));
        assert_eq!(resolved.as_deref(), Some("gateway.example.com"));
    }

    /// 验证首条流消息发送策略，确保空请求体不会被误发送。
    #[test]
    fn test_should_send_initial_stream_message_strategy() {
        assert!(should_send_initial_stream_message(
            GrpcStreamType::ServerStreaming,
            "{}"
        ));
        assert!(should_send_initial_stream_message(
            GrpcStreamType::ClientStreaming,
            r#"{"name":"jack"}"#
        ));
        assert!(should_send_initial_stream_message(
            GrpcStreamType::Bidirectional,
            r#"{"name":"jack"}"#
        ));

        assert!(!should_send_initial_stream_message(
            GrpcStreamType::ClientStreaming,
            "   "
        ));
    }

}
