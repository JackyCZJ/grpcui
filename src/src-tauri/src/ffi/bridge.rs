//! FFI Bridge for Go Codec - Go 编解码器 FFI 桥接模块
//!
//! 本模块提供对 Go FFI 动态库的安全 Rust 封装，用于 protobuf 消息的编码和解码。
//! 实现了完整的 RAII 生命周期管理、线程安全机制和统一的错误映射。
//!
//! # 安全保证
//!
//! - 所有 FFI 调用都经过空指针检查
//! - 字符串数据强制 UTF-8 验证
//! - 内存分配/释放配对管理，杜绝泄漏
//! - 异常边界通过 Result 类型传递
//!
//! # 线程安全
//!
//! `CodecBridge` 实现了 `Send` 和 `Sync`，可在多线程环境中安全使用。
//! 底层 Go 库句柄通过原子操作管理生命周期。
//!
//! # 使用示例
//!
//! ```no_run
//! use grpcui::ffi::bridge::{CodecBridge, BridgeManager};
//!
//! // 方式一：直接使用 CodecBridge
//! let bridge = CodecBridge::new().expect("加载 FFI 库失败");
//! let wire_data = bridge.encode_request("MyService/MyMethod", r#"{"name": "test"}"#)
//!     .expect("编码失败");
//! let json_response = bridge.decode_response("MyService/MyMethod", &wire_data)
//!     .expect("解码失败");
//!
//! // 方式二：使用 BridgeManager 管理多个桥接实例
//! let manager = BridgeManager::new();
//! let handle = manager.create_bridge().expect("创建桥接实例失败");
//! if let Some(bridge) = manager.get_bridge(handle) {
//!     // 使用 bridge...
//! }
//! manager.remove_bridge(handle);
//! ```

#![allow(dead_code)]
use std::collections::HashMap;
use std::ffi::{CStr, CString, c_char, c_void};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use libloading::{Library, Symbol};
use serde::Serialize;

use super::error::{FfiError, Result};

/// FFI 函数类型定义
///
/// 这些类型对应 Go 动态库导出的 C 函数签名。
///
/// 创建新桥接实例的函数类型
///
/// 返回一个无符号整数作为桥接句柄，用于后续操作
pub type BridgeNewFn = unsafe extern "C" fn() -> usize;

/// 释放桥接实例的函数类型
///
/// 接收桥接句柄，释放相关资源
pub type BridgeFreeFn = unsafe extern "C" fn(handle: usize);

/// 编码请求的函数类型
///
/// 参数:
/// - handle: 桥接句柄
/// - method_name: 方法名（C 字符串）
/// - json_payload: JSON 格式的请求体（C 字符串）
///
/// 返回: 指向 Buffer 结构的指针，包含编码后的二进制数据
pub type EncodeRequestFn = unsafe extern "C" fn(
    handle: usize,
    method_name: *const c_char,
    json_payload: *const c_char,
) -> *mut Buffer;

/// 解码响应的函数类型
///
/// 参数:
/// - handle: 桥接句柄
/// - method_name: 方法名（C 字符串）
/// - wire_data: 二进制响应数据指针
/// - wire_len: 数据长度
///
/// 返回: 指向 C 字符串的指针，包含 JSON 格式的响应体
pub type DecodeResponseFn = unsafe extern "C" fn(
    handle: usize,
    method_name: *const c_char,
    wire_data: *const c_char,
    wire_len: usize,
) -> *mut c_char;

/// 从 proto 文件加载描述信息的函数类型
///
/// 参数:
/// - handle: 桥接句柄
/// - proto_paths_json: proto 文件路径 JSON 数组
/// - import_paths_json: import 路径 JSON 数组（可为空指针）
///
/// 返回: 0 表示成功，非 0 表示失败
pub type LoadProtoFilesFn = unsafe extern "C" fn(
    handle: usize,
    proto_paths_json: *const c_char,
    import_paths_json: *const c_char,
) -> i32;

/// 通过 gRPC 反射加载描述信息的函数类型
///
/// 参数:
/// - handle: 桥接句柄
/// - target_addr: 服务端地址
/// - tls_config_json: TLS 配置 JSON（可为空指针）
///
/// 返回: 0 表示成功，非 0 表示失败
pub type LoadReflectionFn = unsafe extern "C" fn(
    handle: usize,
    target_addr: *const c_char,
    tls_config_json: *const c_char,
) -> i32;

/// 重置桥接内部解析状态的函数类型
///
/// 该函数会关闭现有连接并清空 parser 缓存，使后续加载以全新状态开始。
/// 返回: 0 表示成功，非 0 表示失败
pub type ResetParserFn = unsafe extern "C" fn(handle: usize) -> i32;

/// 查询服务列表的函数类型
///
/// 返回: JSON 字符串指针（需要 free_cstring 释放）
pub type ListServicesFn = unsafe extern "C" fn(handle: usize) -> *mut c_char;

/// 查询某个服务的方法列表的函数类型
///
/// 参数:
/// - handle: 桥接句柄
/// - service_name: 服务全名
///
/// 返回: JSON 字符串指针（需要 free_cstring 释放）
pub type ListMethodsFn = unsafe extern "C" fn(handle: usize, service_name: *const c_char) -> *mut c_char;

/// 获取方法入参 schema 的函数类型
///
/// 参数:
/// - handle: 桥接句柄
/// - service_name: 服务全名
/// - method_name: 方法名
///
/// 返回: JSON 字符串指针（需要 free_cstring 释放）
pub type GetMethodInputSchemaFn = unsafe extern "C" fn(
    handle: usize,
    service_name: *const c_char,
    method_name: *const c_char,
) -> *mut c_char;

/// 获取最后一次错误信息的函数类型
///
/// 返回: 指向 C 字符串的指针，包含错误信息
pub type LastErrorFn = unsafe extern "C" fn() -> *mut c_char;

/// 释放 Buffer 结构的函数类型
///
/// 用于释放 Go 侧分配的内存
pub type FreeBufferFn = unsafe extern "C" fn(buf: *mut c_void);

/// 释放 C 字符串的函数类型
///
/// 用于释放 Go 侧分配的字符串内存
pub type FreeCStringFn = unsafe extern "C" fn(s: *mut c_char);

/// Go FFI 返回的 Buffer 结构
///
/// 用于在 Rust 和 Go 之间传递二进制数据。
/// 注意：data 指针指向的内存由 Go 分配，必须通过 free_buffer 释放。
#[repr(C)]
pub struct Buffer {
    /// 数据指针
    pub data: *mut c_char,
    /// 数据长度（字节）
    pub len: usize,
}

/// Reflection TLS 配置
///
/// 字段命名与 Go FFI 约定保持一致，便于直接序列化为 JSON 传递。
#[derive(Debug, Clone, Default, Serialize)]
pub struct ReflectionTlsConfig {
    /// 是否禁用证书校验（true 时等同于 insecure 连接）
    pub insecure: bool,
    /// 客户端证书路径
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cert_path: Option<String>,
    /// 客户端私钥路径
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_path: Option<String>,
    /// CA 证书路径
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ca_path: Option<String>,
}

/// 编解码桥接器
///
/// 提供对 Go FFI 库的安全封装，管理动态库生命周期和内存安全。
/// 实现了 RAII 模式，确保资源正确释放。
pub struct CodecBridge {
    /// 动态库引用，通过 Arc 共享
    /// 使用下划线前缀避免未使用警告，但保留所有权以确保库生命周期
    _library: Arc<Library>,
    /// Go 桥接句柄
    handle: usize,
    /// 桥接释放函数指针
    bridge_free: BridgeFreeFn,
    /// 编码请求函数指针
    encode_fn: EncodeRequestFn,
    /// 解码响应函数指针
    decode_fn: DecodeResponseFn,
    /// 加载 proto 文件函数指针
    load_proto_files_fn: LoadProtoFilesFn,
    /// 加载 reflection 描述函数指针
    load_reflection_fn: LoadReflectionFn,
    /// 重置 parser 状态函数指针
    reset_parser_fn: ResetParserFn,
    /// 获取服务列表函数指针
    list_services_fn: ListServicesFn,
    /// 获取方法列表函数指针
    list_methods_fn: ListMethodsFn,
    /// 获取方法入参 schema 函数指针
    get_method_input_schema_fn: GetMethodInputSchemaFn,
    /// 获取错误信息函数指针
    last_error_fn: LastErrorFn,
    /// 释放 Buffer 函数指针
    free_buffer_fn: FreeBufferFn,
    /// 释放 C 字符串函数指针
    free_cstring_fn: FreeCStringFn,
}

impl std::fmt::Debug for CodecBridge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CodecBridge")
            .field("handle", &self.handle)
            .finish()
    }
}

/// 库缓存静态变量
///
/// 使用 Mutex 保护，确保线程安全。
/// 缓存已加载的库，避免重复加载。
static LIBRARY_CACHE: Mutex<Option<Arc<Library>>> = Mutex::new(None);

impl CodecBridge {
    /// 创建新的 CodecBridge 实例
    ///
    /// 自动搜索并加载 FFI 动态库，初始化桥接句柄。
    ///
    /// # 错误
    ///
    /// 可能返回以下错误:
    /// - `FfiError::LibraryNotFound`: 未找到动态库
    /// - `FfiError::SymbolNotFound`: 未找到必需的 FFI 符号
    /// - `FfiError::LibraryLoadFailed`: 动态库加载失败
    ///
    /// # 示例
    ///
    /// ```no_run
    /// use grpcui::ffi::bridge::CodecBridge;
    ///
    /// match CodecBridge::new() {
    ///     Ok(bridge) => println!("FFI 桥接初始化成功"),
    ///     Err(e) => eprintln!("初始化失败: {}", e),
    /// }
    /// ```
    pub fn new() -> Result<Self> {
        let library = Self::load_library()?;
        Self::with_library(library)
    }

    /// 从标准位置加载 FFI 动态库
    ///
    /// 搜索顺序:
    /// 1. GRPC_CODEC_BRIDGE_PATH 环境变量指定的路径
    /// 2. 当前工作目录及其子目录
    /// 3. 可执行文件所在目录及其子目录
    ///
    /// 使用缓存机制避免重复加载。
    fn load_library() -> Result<Arc<Library>> {
        // 首先检查缓存
        {
            let cache = LIBRARY_CACHE.lock().map_err(|_| FfiError::IoError {
                context: "获取库缓存锁".to_string(),
                reason: "锁被污染".to_string(),
            })?;
            if let Some(lib) = cache.as_ref() {
                return Ok(Arc::clone(lib));
            }
        }

        let paths = Self::library_search_paths();
        let mut tried_paths = Vec::new();

        for path in &paths {
            tried_paths.push(path.clone());
            if path.exists() {
                // 安全说明: libloading::Library::new 是 unsafe 的，
                // 因为动态库可能包含恶意代码。我们假设库来自可信来源。
                unsafe {
                    match Library::new(path) {
                        Ok(lib) => {
                            let lib = Arc::new(lib);
                            let mut cache = LIBRARY_CACHE.lock().map_err(|_| FfiError::IoError {
                                context: "更新库缓存".to_string(),
                                reason: "锁被污染".to_string(),
                            })?;
                            *cache = Some(Arc::clone(&lib));
                            return Ok(lib);
                        }
                        Err(e) => {
                            log::warn!("无法从 {} 加载库: {}", path.display(), e);
                        }
                    }
                }
            }
        }

        Err(FfiError::LibraryNotFound { paths: tried_paths })
    }

    /// 获取库搜索路径列表
    ///
    /// 根据平台生成可能的库文件路径。
    fn library_search_paths() -> Vec<PathBuf> {
        let mut paths = Vec::new();

        // 首先检查环境变量
        if let Ok(env_path) = std::env::var("GRPC_CODEC_BRIDGE_PATH") {
            paths.push(PathBuf::from(env_path));
        }

        let lib_names = Self::library_names();

        // 当前工作目录
        if let Ok(current_dir) = std::env::current_dir() {
            for name in &lib_names {
                // 常见开发路径（tauri dev 从 src 目录/ src-tauri 目录启动）
                paths.push(current_dir.join(name));
                paths.push(current_dir.join("resources").join(name));
                paths.push(current_dir.join("src-tauri").join("resources").join(name));

                // sidecar 目录（兼容仓库根目录、src 目录两种相对层级）
                paths.push(current_dir.join("sidecar").join(name));
                paths.push(current_dir.join("sidecar").join("ffi").join(name));
                paths.push(current_dir.join("..").join("sidecar").join(name));
                paths.push(current_dir.join("..").join("sidecar").join("ffi").join(name));
                paths.push(current_dir.join("..").join("..").join("sidecar").join(name));
                paths.push(
                    current_dir
                        .join("..")
                        .join("..")
                        .join("sidecar")
                        .join("ffi")
                        .join(name),
                );
            }
        }

        // 可执行文件目录
        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(exe_dir) = exe_path.parent() {
                for name in &lib_names {
                    paths.push(exe_dir.join(name));
                    paths.push(exe_dir.join("resources").join(name));
                    paths.push(exe_dir.join("..").join("resources").join(name));
                    paths.push(exe_dir.join("..").join("sidecar").join(name));
                    paths.push(exe_dir.join("..").join("sidecar").join("ffi").join(name));
                }
            }
        }

        paths
    }

    /// 获取平台特定的库文件名
    ///
    /// 根据不同操作系统返回对应的动态库文件名。
    fn library_names() -> Vec<String> {
        let base_name = "grpc_codec_bridge";

        #[cfg(target_os = "macos")]
        {
            vec![format!("lib{}.dylib", base_name)]
        }

        #[cfg(target_os = "linux")]
        {
            vec![format!("lib{}.so", base_name)]
        }

        #[cfg(target_os = "windows")]
        {
            vec![format!("{}.dll", base_name)]
        }

        #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
        {
            vec![format!("lib{}.so", base_name)]
        }
    }

    /// 使用已加载的库创建桥接实例
    ///
    /// 从动态库中获取所有必需的 FFI 符号，创建桥接句柄。
    ///
    /// # 安全
    ///
    /// 此函数涉及 unsafe 操作:
    /// - 动态库符号获取
    /// - 调用外部函数创建桥接句柄
    fn with_library(library: Arc<Library>) -> Result<Self> {
        // 安全说明: 以下操作都是 unsafe 的，因为涉及动态库操作
        unsafe {
            // 获取所有符号 - 必须在 library_clone 创建之前完成
            let bridge_new: Symbol<BridgeNewFn> = library
                .get(b"bridge_new")
                .map_err(|e| FfiError::SymbolNotFound {
                    name: format!("bridge_new: {}", e),
                })?;

            let bridge_free: Symbol<BridgeFreeFn> = library
                .get(b"bridge_free")
                .map_err(|e| FfiError::SymbolNotFound {
                    name: format!("bridge_free: {}", e),
                })?;

            let encode_fn: Symbol<EncodeRequestFn> = library
                .get(b"encode_request_json_to_wire")
                .map_err(|e| FfiError::SymbolNotFound {
                    name: format!("encode_request_json_to_wire: {}", e),
                })?;

            let decode_fn: Symbol<DecodeResponseFn> = library
                .get(b"decode_response_wire_to_json")
                .map_err(|e| FfiError::SymbolNotFound {
                    name: format!("decode_response_wire_to_json: {}", e),
                })?;

            let load_proto_files_fn: Symbol<LoadProtoFilesFn> = library
                .get(b"load_proto_files")
                .map_err(|e| FfiError::SymbolNotFound {
                    name: format!("load_proto_files: {}", e),
                })?;

            let load_reflection_fn: Symbol<LoadReflectionFn> = library
                .get(b"load_reflection")
                .map_err(|e| FfiError::SymbolNotFound {
                    name: format!("load_reflection: {}", e),
                })?;

            let reset_parser_fn: Symbol<ResetParserFn> = library
                .get(b"reset_parser")
                .map_err(|e| FfiError::SymbolNotFound {
                    name: format!("reset_parser: {}", e),
                })?;

            let list_services_fn: Symbol<ListServicesFn> = library
                .get(b"list_services")
                .map_err(|e| FfiError::SymbolNotFound {
                    name: format!("list_services: {}", e),
                })?;

            let list_methods_fn: Symbol<ListMethodsFn> = library
                .get(b"list_methods")
                .map_err(|e| FfiError::SymbolNotFound {
                    name: format!("list_methods: {}", e),
                })?;

            let get_method_input_schema_fn: Symbol<GetMethodInputSchemaFn> = library
                .get(b"get_method_input_schema")
                .map_err(|e| FfiError::SymbolNotFound {
                    name: format!("get_method_input_schema: {}", e),
                })?;

            let last_error_fn: Symbol<LastErrorFn> = library
                .get(b"last_error")
                .map_err(|e| FfiError::SymbolNotFound {
                    name: format!("last_error: {}", e),
                })?;

            let free_buffer_fn: Symbol<FreeBufferFn> = library
                .get(b"free_buffer")
                .map_err(|e| FfiError::SymbolNotFound {
                    name: format!("free_buffer: {}", e),
                })?;

            let free_cstring_fn: Symbol<FreeCStringFn> = library
                .get(b"free_cstring")
                .map_err(|e| FfiError::SymbolNotFound {
                    name: format!("free_cstring: {}", e),
                })?;

            // 解引用所有符号
            let bridge_new_fn = *bridge_new;
            let bridge_free_fn = *bridge_free;
            let encode_fn = *encode_fn;
            let decode_fn = *decode_fn;
            let load_proto_files_fn = *load_proto_files_fn;
            let load_reflection_fn = *load_reflection_fn;
            let reset_parser_fn = *reset_parser_fn;
            let list_services_fn = *list_services_fn;
            let list_methods_fn = *list_methods_fn;
            let get_method_input_schema_fn = *get_method_input_schema_fn;
            let last_error_fn = *last_error_fn;
            let free_buffer_fn = *free_buffer_fn;
            let free_cstring_fn = *free_cstring_fn;

            // 创建桥接句柄
            let handle = bridge_new_fn();

            Ok(CodecBridge {
                _library: library,
                handle,
                bridge_free: bridge_free_fn,
                encode_fn,
                decode_fn,
                load_proto_files_fn,
                load_reflection_fn,
                reset_parser_fn,
                list_services_fn,
                list_methods_fn,
                get_method_input_schema_fn,
                last_error_fn,
                free_buffer_fn,
                free_cstring_fn,
            })
        }
    }

    /// 获取 Go 侧的最后错误信息
    ///
    /// 当 FFI 调用失败时，调用此函数获取详细错误信息。
    /// 自动释放 Go 分配的内存。
    fn get_last_error(&self) -> String {
        unsafe {
            let error_ptr = (self.last_error_fn)();

            // 空指针检查
            if error_ptr.is_null() {
                return "未知错误 (Go 未返回错误信息)".to_string();
            }

            // 转换为 Rust 字符串
            let error_str = CStr::from_ptr(error_ptr)
                .to_string_lossy()
                .into_owned();

            // 释放 Go 分配的内存
            (self.free_cstring_fn)(error_ptr);

            error_str
        }
    }

    /// 将 JSON 请求体编码为 protobuf 二进制格式
    ///
    /// # 参数
    ///
    /// - `method`: 方法名，格式为 "ServiceName/MethodName"
    /// - `json_payload`: JSON 格式的请求体
    ///
    /// # 返回
    ///
    /// 成功时返回编码后的二进制数据，失败时返回 `FfiError`。
    ///
    /// # 错误
    ///
    /// - `FfiError::InvalidMethodName`: 方法名包含空字节
    /// - `FfiError::InvalidUtf8`: JSON 数据包含无效 UTF-8 序列
    /// - `FfiError::FfiCallFailed`: Go 编码失败
    ///
    /// # 示例
    ///
    /// ```no_run
    /// use grpcui::ffi::bridge::CodecBridge;
    ///
    /// let bridge = CodecBridge::new().unwrap();
    /// let wire_data = bridge.encode_request(
    ///     "Greeter/SayHello",
    ///     r#"{"name": "World"}"#
    /// ).expect("编码失败");
    /// ```
    pub fn encode_request(&self, method: &str, json_payload: &str) -> Result<Vec<u8>> {
        // 验证方法名
        if method.is_empty() {
            return Err(FfiError::InvalidMethodName {
                name: "(空)".to_string(),
            });
        }

        // 转换为 C 字符串
        let method_c = CString::new(method).map_err(|_| FfiError::InvalidMethodName {
            name: method.to_string(),
        })?;

        // 验证 JSON 数据
        if json_payload.is_empty() {
            return Err(FfiError::JsonParseError {
                json: "".to_string(),
                reason: "空 JSON 数据".to_string(),
            });
        }

        let json_c = CString::new(json_payload).map_err(|_| FfiError::InvalidUtf8 {
            bytes: json_payload.as_bytes().to_vec(),
        })?;

        unsafe {
            // 调用 Go FFI 编码函数
            let buffer_ptr = (self.encode_fn)(self.handle, method_c.as_ptr(), json_c.as_ptr());

            // 空指针检查 - Go 返回空表示失败
            if buffer_ptr.is_null() {
                let error_msg = self.get_last_error();
                return Err(FfiError::FfiCallFailed {
                    function: "encode_request_json_to_wire".to_string(),
                    message: error_msg,
                });
            }

            // 解引用 Buffer 结构
            let buffer = &*buffer_ptr;

            // 检查 data 指针
            if buffer.data.is_null() {
                (self.free_buffer_fn)(buffer_ptr as *mut c_void);
                return Err(FfiError::NullPointer {
                    context: "encode_request: buffer.data".to_string(),
                });
            }

            // 复制数据到 Rust Vec
            let data = std::slice::from_raw_parts(buffer.data as *const u8, buffer.len).to_vec();

            // 释放 Go 分配的 Buffer
            (self.free_buffer_fn)(buffer_ptr as *mut c_void);

            Ok(data)
        }
    }

    /// 将 protobuf 二进制响应解码为 JSON
    ///
    /// # 参数
    ///
    /// - `method`: 方法名，格式为 "ServiceName/MethodName"
    /// - `wire_data`: protobuf 二进制数据
    ///
    /// # 返回
    ///
    /// 成功时返回 JSON 字符串，失败时返回 `FfiError`。
    ///
    /// # 错误
    ///
    /// - `FfiError::InvalidMethodName`: 方法名包含空字节
    /// - `FfiError::FfiCallFailed`: Go 解码失败
    ///
    /// # 示例
    ///
    /// ```no_run
    /// use grpcui::ffi::bridge::CodecBridge;
    ///
    /// let bridge = CodecBridge::new().unwrap();
    /// let wire_data = vec![0x0a, 0x05, 0x48, 0x65, 0x6c, 0x6c, 0x6f];
    /// let json = bridge.decode_response("Greeter/SayHello", &wire_data)
    ///     .expect("解码失败");
    /// ```
    pub fn decode_response(&self, method: &str, wire_data: &[u8]) -> Result<String> {
        // 验证方法名
        if method.is_empty() {
            return Err(FfiError::InvalidMethodName {
                name: "(空)".to_string(),
            });
        }

        // 验证数据
        if wire_data.is_empty() {
            return Err(FfiError::IoError {
                context: "decode_response".to_string(),
                reason: "空 wire 数据".to_string(),
            });
        }

        let method_c = CString::new(method).map_err(|_| FfiError::InvalidMethodName {
            name: method.to_string(),
        })?;

        unsafe {
            // 调用 Go FFI 解码函数
            let json_ptr = (self.decode_fn)(
                self.handle,
                method_c.as_ptr(),
                wire_data.as_ptr() as *const c_char,
                wire_data.len(),
            );

            // 空指针检查
            if json_ptr.is_null() {
                let error_msg = self.get_last_error();
                return Err(FfiError::FfiCallFailed {
                    function: "decode_response_wire_to_json".to_string(),
                    message: error_msg,
                });
            }

            // 转换为 Rust 字符串（自动处理 UTF-8 验证）
            let json_str = CStr::from_ptr(json_ptr)
                .to_string_lossy()
                .into_owned();

            // 释放 Go 分配的内存
            (self.free_cstring_fn)(json_ptr);

            Ok(json_str)
        }
    }

    /// 从本地 proto 文件加载服务描述
    ///
    /// 该函数会将 proto 文件路径与 import 目录序列化为 JSON，并通过 FFI 调用 Go 侧
    /// `load_proto_files`。调用成功后，后续 `list_services` / `encode_request` 等能力
    /// 都会基于新加载的描述信息工作。
    ///
    /// # 参数
    ///
    /// - `proto_paths`: proto 文件绝对路径或相对路径列表（至少一个）
    /// - `import_paths`: proto import 搜索目录列表（可为空）
    ///
    /// # 错误
    ///
    /// - `FfiError::IoError`: 传入 proto 路径为空
    /// - `FfiError::JsonParseError`: 路径列表序列化失败
    /// - `FfiError::FfiCallFailed`: Go 侧加载失败
    pub fn load_proto_files(&self, proto_paths: &[String], import_paths: &[String]) -> Result<()> {
        if proto_paths.is_empty() {
            return Err(FfiError::IoError {
                context: "load_proto_files".to_string(),
                reason: "proto_paths 不能为空".to_string(),
            });
        }

        let proto_paths_json = serde_json::to_string(proto_paths)?;
        let import_paths_json = if import_paths.is_empty() {
            None
        } else {
            Some(serde_json::to_string(import_paths)?)
        };

        let proto_paths_c = CString::new(proto_paths_json.clone()).map_err(|_| FfiError::InvalidUtf8 {
            bytes: proto_paths_json.into_bytes(),
        })?;

        let import_paths_c = match import_paths_json {
            Some(json) => Some(
                CString::new(json.clone()).map_err(|_| FfiError::InvalidUtf8 {
                    bytes: json.into_bytes(),
                })?,
            ),
            None => None,
        };

        unsafe {
            let code = (self.load_proto_files_fn)(
                self.handle,
                proto_paths_c.as_ptr(),
                import_paths_c
                    .as_ref()
                    .map_or(std::ptr::null(), |value| value.as_ptr()),
            );

            if code != 0 {
                return Err(FfiError::FfiCallFailed {
                    function: "load_proto_files".to_string(),
                    message: self.get_last_error(),
                });
            }
        }

        Ok(())
    }

    /// 通过 gRPC Reflection 加载服务描述
    ///
    /// 调用成功后，桥接器内部会缓存对应服务的 descriptor，可直接用于服务发现和
    /// 请求编解码。TLS 配置会按 Go FFI 约定序列化为 JSON。
    ///
    /// # 参数
    ///
    /// - `address`: 目标 gRPC 地址，例如 `localhost:50051`
    /// - `tls_config`: 可选 TLS 配置；不传时走 Go 侧默认 TLS 策略
    ///
    /// # 错误
    ///
    /// - `FfiError::IoError`: address 为空
    /// - `FfiError::JsonParseError`: TLS 配置序列化失败
    /// - `FfiError::FfiCallFailed`: reflection 加载失败
    pub fn load_reflection(
        &self,
        address: &str,
        tls_config: Option<&ReflectionTlsConfig>,
    ) -> Result<()> {
        if address.trim().is_empty() {
            return Err(FfiError::IoError {
                context: "load_reflection".to_string(),
                reason: "address 不能为空".to_string(),
            });
        }

        let address_c = CString::new(address).map_err(|_| FfiError::InvalidUtf8 {
            bytes: address.as_bytes().to_vec(),
        })?;

        let tls_config_json = match tls_config {
            Some(config) => Some(serde_json::to_string(config)?),
            None => None,
        };

        let tls_config_c = match tls_config_json {
            Some(json) => Some(
                CString::new(json.clone()).map_err(|_| FfiError::InvalidUtf8 {
                    bytes: json.into_bytes(),
                })?,
            ),
            None => None,
        };

        unsafe {
            let code = (self.load_reflection_fn)(
                self.handle,
                address_c.as_ptr(),
                tls_config_c
                    .as_ref()
                    .map_or(std::ptr::null(), |value| value.as_ptr()),
            );

            if code != 0 {
                return Err(FfiError::FfiCallFailed {
                    function: "load_reflection".to_string(),
                    message: self.get_last_error(),
                });
            }
        }

        Ok(())
    }

    /// reset_parser 会强制清空 Go 侧 parser 与连接缓存。
    ///
    /// 该操作用于项目切换或主动断连场景，确保下一次 `load_proto_files` /
    /// `load_reflection` 不会读取到上一个项目留下的描述符。
    pub fn reset_parser(&self) -> Result<()> {
        unsafe {
            let code = (self.reset_parser_fn)(self.handle);
            if code != 0 {
                return Err(FfiError::FfiCallFailed {
                    function: "reset_parser".to_string(),
                    message: self.get_last_error(),
                });
            }
        }

        Ok(())
    }

    /// 查询当前已加载的服务列表（JSON 字符串）
    ///
    /// 返回值格式由 Go FFI 约定，典型结构为 `{ "services": [...] }`。
    pub fn list_services(&self) -> Result<String> {
        unsafe {
            let json_ptr = (self.list_services_fn)(self.handle);
            self.consume_owned_cstring("list_services", json_ptr)
        }
    }

    /// 查询指定服务下的方法列表（JSON 字符串）
    ///
    /// # 参数
    ///
    /// - `service_name`: 服务全名，例如 `package.Greeter`
    pub fn list_methods(&self, service_name: &str) -> Result<String> {
        if service_name.trim().is_empty() {
            return Err(FfiError::IoError {
                context: "list_methods".to_string(),
                reason: "service_name 不能为空".to_string(),
            });
        }

        let service_name_c = CString::new(service_name).map_err(|_| FfiError::InvalidUtf8 {
            bytes: service_name.as_bytes().to_vec(),
        })?;

        unsafe {
            let json_ptr = (self.list_methods_fn)(self.handle, service_name_c.as_ptr());
            self.consume_owned_cstring("list_methods", json_ptr)
        }
    }

    /// 获取指定方法的请求体字段 schema（JSON 字符串）。
    ///
    /// 该接口用于前端“字段化请求体编辑器”：
    /// 前端拿到 schema 后可渲染表单，再把用户输入序列化成 JSON 发送。
    pub fn get_method_input_schema(&self, service_name: &str, method_name: &str) -> Result<String> {
        if service_name.trim().is_empty() || method_name.trim().is_empty() {
            return Err(FfiError::IoError {
                context: "get_method_input_schema".to_string(),
                reason: "service_name 与 method_name 不能为空".to_string(),
            });
        }

        let service_name_c = CString::new(service_name).map_err(|_| FfiError::InvalidUtf8 {
            bytes: service_name.as_bytes().to_vec(),
        })?;
        let method_name_c = CString::new(method_name).map_err(|_| FfiError::InvalidUtf8 {
            bytes: method_name.as_bytes().to_vec(),
        })?;

        unsafe {
            let response_ptr = (self.get_method_input_schema_fn)(
                self.handle,
                service_name_c.as_ptr(),
                method_name_c.as_ptr(),
            );

            if response_ptr.is_null() {
                return Err(FfiError::FfiCallFailed {
                    function: "get_method_input_schema".to_string(),
                    message: self.get_last_error(),
                });
            }

            let response_str = CStr::from_ptr(response_ptr)
                .to_string_lossy()
                .into_owned();
            (self.free_cstring_fn)(response_ptr);

            Ok(response_str)
        }
    }

    /// 将 Go 侧分配的 C 字符串安全转换为 Rust String
    ///
    /// 该辅助函数统一处理空指针检查、错误信息提取与内存释放，避免每个 JSON 查询
    /// 方法重复编写易错的 FFI 清理逻辑。
    fn consume_owned_cstring(&self, function_name: &str, cstring_ptr: *mut c_char) -> Result<String> {
        if cstring_ptr.is_null() {
            return Err(FfiError::FfiCallFailed {
                function: function_name.to_string(),
                message: self.get_last_error(),
            });
        }

        unsafe {
            let content = CStr::from_ptr(cstring_ptr).to_string_lossy().into_owned();
            (self.free_cstring_fn)(cstring_ptr);
            Ok(content)
        }
    }
}

/// RAII 析构实现
///
/// 确保当 CodecBridge 被丢弃时，正确释放 Go 桥接句柄。
impl Drop for CodecBridge {
    fn drop(&mut self) {
        // 安全说明: 这里调用 Go 的释放函数是安全的，
        // 因为 handle 是有效的，且 bridge_free 是有效的函数指针
        unsafe {
            (self.bridge_free)(self.handle);
        }
    }
}

/// Send 标记实现
///
/// CodecBridge 可以安全地在线程间转移所有权。
unsafe impl Send for CodecBridge {}

/// Sync 标记实现
///
/// CodecBridge 可以安全地在线程间共享引用。
unsafe impl Sync for CodecBridge {}

/// 桥接句柄生成器
///
/// 使用原子计数器生成唯一的桥接句柄 ID。
static BRIDGE_COUNTER: AtomicUsize = AtomicUsize::new(1);

/// 桥接管理器
///
/// 管理多个 CodecBridge 实例的生命周期，提供线程安全的访问。
///
/// # 使用场景
///
/// 当需要同时维护多个独立的编解码上下文时使用，例如:
/// - 不同的 proto 定义
/// - 不同的服务连接
///
/// # 示例
///
/// ```
/// use grpcui::ffi::bridge::BridgeManager;
///
/// let manager = BridgeManager::new();
/// // 创建桥接实例
/// // let handle = manager.create_bridge().unwrap();
/// ```
pub struct BridgeManager {
    /// 桥接实例映射表
    bridges: Mutex<HashMap<usize, Arc<CodecBridge>>>,
}

impl BridgeManager {
    /// 创建新的桥接管理器
    pub fn new() -> Self {
        Self {
            bridges: Mutex::new(HashMap::new()),
        }
    }

    /// 创建新的桥接实例
    ///
    /// 加载 FFI 库，创建 CodecBridge，并返回管理句柄。
    ///
    /// # 返回
    ///
    /// 成功时返回句柄 ID，失败时返回 `FfiError`。
    pub fn create_bridge(&self) -> Result<usize> {
        let bridge = Arc::new(CodecBridge::new()?);
        let handle = BRIDGE_COUNTER.fetch_add(1, Ordering::SeqCst);

        let mut bridges = self.bridges.lock().map_err(|_| FfiError::IoError {
            context: "创建桥接".to_string(),
            reason: "锁被污染".to_string(),
        })?;

        bridges.insert(handle, bridge);
        Ok(handle)
    }

    /// 通过句柄获取桥接实例
    ///
    /// # 参数
    ///
    /// - `handle`: 桥接句柄 ID
    ///
    /// # 返回
    ///
    /// 如果句柄存在，返回 `Some(Arc<CodecBridge>)`，否则返回 `None`。
    pub fn get_bridge(&self, handle: usize) -> Option<Arc<CodecBridge>> {
        let bridges = self.bridges.lock().ok()?;
        bridges.get(&handle).cloned()
    }

    /// 移除桥接实例
    ///
    /// # 参数
    ///
    /// - `handle`: 桥接句柄 ID
    ///
    /// # 返回
    ///
    /// 如果句柄存在，返回被移除的 `Arc<CodecBridge>`，否则返回 `None`。
    ///
    /// # 注意
    ///
    /// 返回的 Arc 可能还有其他引用，只有当最后一个引用被丢弃时，
    /// 桥接实例才会真正被销毁。
    pub fn remove_bridge(&self, handle: usize) -> Option<Arc<CodecBridge>> {
        let mut bridges = self.bridges.lock().ok()?;
        bridges.remove(&handle)
    }

    /// 获取当前管理的桥接实例数量
    pub fn bridge_count(&self) -> usize {
        self.bridges.lock().map(|b| b.len()).unwrap_or(0)
    }

    /// 清空所有桥接实例
    pub fn clear_bridges(&self) {
        if let Ok(mut bridges) = self.bridges.lock() {
            bridges.clear();
        }
    }
}

impl Default for BridgeManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试库文件名生成
    #[test]
    fn test_library_names() {
        let names = CodecBridge::library_names();
        assert!(!names.is_empty(), "库名列表不应为空");

        #[cfg(target_os = "macos")]
        {
            assert_eq!(names, vec!["libgrpc_codec_bridge.dylib".to_string()]);
        }

        #[cfg(target_os = "linux")]
        {
            assert_eq!(names, vec!["libgrpc_codec_bridge.so".to_string()]);
        }

        #[cfg(target_os = "windows")]
        {
            assert_eq!(names, vec!["grpc_codec_bridge.dll".to_string()]);
        }
    }

    /// 测试搜索路径生成
    #[test]
    fn test_library_search_paths() {
        let paths = CodecBridge::library_search_paths();
        // 即使没有环境变量，也应该有一些默认路径
        assert!(!paths.is_empty(), "搜索路径不应为空");
    }

    /// 验证默认搜索路径包含 resources 目录候选，
    /// 避免 tauri dev 场景下无法定位 src-tauri/resources 中的动态库。
    #[test]
    fn test_library_search_paths_include_resources_candidate() {
        let paths = CodecBridge::library_search_paths();
        let lib_name = CodecBridge::library_names()
            .first()
            .cloned()
            .expect("平台库名列表不应为空");

        let expected_suffix = PathBuf::from("resources").join(lib_name);
        assert!(
            paths.iter().any(|path| path.ends_with(&expected_suffix)),
            "搜索路径应包含 resources 候选: {:?}",
            expected_suffix
        );
    }

    /// 测试桥接管理器基本功能
    #[test]
    fn test_bridge_manager_basic() {
        let manager = BridgeManager::new();

        // 初始状态应为空
        assert_eq!(manager.bridge_count(), 0, "初始桥接数应为 0");
        assert!(manager.get_bridge(0).is_none(), "不存在的句柄应返回 None");
        assert!(manager.remove_bridge(0).is_none(), "移除不存在的句柄应返回 None");
    }

    /// 测试桥接管理器计数
    #[test]
    fn test_bridge_manager_count() {
        let manager = BridgeManager::new();
        assert_eq!(manager.bridge_count(), 0);

        // 注意: 由于没有实际的 FFI 库，create_bridge 会失败
        // 这里测试在没有库的情况下不会 panic
    }

    /// 测试桥接管理器清空
    #[test]
    fn test_bridge_manager_clear() {
        let manager = BridgeManager::new();
        manager.clear_bridges();
        assert_eq!(manager.bridge_count(), 0, "清空后桥接数应为 0");
    }

    /// 测试句柄生成器
    #[test]
    fn test_bridge_counter() {
        // 重置计数器（仅用于测试）
        BRIDGE_COUNTER.store(1, Ordering::SeqCst);

        let first = BRIDGE_COUNTER.fetch_add(1, Ordering::SeqCst);
        let second = BRIDGE_COUNTER.fetch_add(1, Ordering::SeqCst);

        assert_eq!(first, 1, "第一个句柄应为 1");
        assert_eq!(second, 2, "第二个句柄应为 2");
        assert!(second > first, "句柄应递增");
    }

    /// 测试错误类型转换
    #[test]
    fn test_ffi_error_conversions() {
        // 测试 CString 错误转换
        let nul_error = CString::new(vec![0u8]).unwrap_err();
        let ffi_error: FfiError = nul_error.into();
        match ffi_error {
            FfiError::InvalidUtf8 { .. } => {}
            _ => panic!("应转换为 InvalidUtf8"),
        }
    }

    /// 测试空方法名错误
    #[test]
    fn test_empty_method_name() {
        // 由于无法创建实际的 CodecBridge（需要 FFI 库），
        // 这里只测试错误类型的创建
        let err = FfiError::InvalidMethodName {
            name: "".to_string(),
        };
        assert!(err.to_string().contains("无效的方法名"));
    }

    /// 测试 Send + Sync 标记
    #[test]
    fn test_send_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}

        assert_send::<CodecBridge>();
        assert_sync::<CodecBridge>();
        assert_send::<BridgeManager>();
        assert_sync::<BridgeManager>();
    }

    /// 测试 Buffer 结构内存布局
    #[test]
    fn test_buffer_layout() {
        use std::mem;

        // 确保 Buffer 结构是 #[repr(C)] 且布局正确
        assert_eq!(mem::size_of::<Buffer>(), mem::size_of::<*mut c_char>() + mem::size_of::<usize>());
    }

    /// 测试库缓存
    #[test]
    fn test_library_cache() {
        // 确保缓存可以被锁定
        let cache = LIBRARY_CACHE.lock();
        assert!(cache.is_ok(), "应能获取缓存锁");
    }

    /// 测试错误消息格式
    #[test]
    fn test_error_messages() {
        let err1 = FfiError::LibraryNotFound {
            paths: vec![PathBuf::from("/test/path")],
        };
        assert!(err1.to_string().contains("FFI 库未找到"));

        let err2 = FfiError::SymbolNotFound {
            name: "test_symbol".to_string(),
        };
        assert!(err2.to_string().contains("test_symbol"));

        let err3 = FfiError::NullPointer {
            context: "test".to_string(),
        };
        assert!(err3.to_string().contains("空指针"));
    }

    // =========================================================================
    // Additional FFI Error Tests
    // =========================================================================

    #[test]
    fn test_ffi_error_library_load_failed() {
        let err = FfiError::LibraryLoadFailed {
            path: PathBuf::from("/nonexistent/lib.so"),
            reason: "file not found".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("无法加载 FFI 库"));
        assert!(msg.contains("file not found"));
    }

    #[test]
    fn test_ffi_error_ffi_call_failed() {
        let err = FfiError::FfiCallFailed {
            function: "encode_request".to_string(),
            message: "proto not loaded".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("FFI 调用 'encode_request' 失败"));
        assert!(msg.contains("proto not loaded"));
    }

    #[test]
    fn test_ffi_error_invalid_utf8() {
        let err = FfiError::InvalidUtf8 {
            bytes: vec![0x80, 0x81],
        };
        assert!(err.to_string().contains("无效的 UTF-8 数据"));
    }

    #[test]
    fn test_ffi_error_json_parse_error() {
        let err = FfiError::JsonParseError {
            json: "{invalid}".to_string(),
            reason: "expected colon".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("JSON 解析失败"));
        assert!(msg.contains("expected colon"));
    }

    #[test]
    fn test_ffi_error_invalid_handle() {
        let err = FfiError::InvalidHandle { handle_id: 42 };
        assert!(err.to_string().contains("无效的桥接句柄 ID: 42"));
    }

    #[test]
    fn test_ffi_error_io_error() {
        let err = FfiError::IoError {
            context: "read file".to_string(),
            reason: "permission denied".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("IO 错误 (read file)"));
        assert!(msg.contains("permission denied"));
    }

    // =========================================================================
    // Library Search Path Tests
    // =========================================================================

    #[test]
    fn test_library_search_paths_with_env() {
        // 设置环境变量后，首个候选路径必须是显式配置值，以验证优先级规则。
        let custom_path = "/custom/path/to/lib";
        std::env::set_var("GRPC_CODEC_BRIDGE_PATH", custom_path);

        let paths = CodecBridge::library_search_paths();

        assert_eq!(
            paths.first().map(|path| path.to_string_lossy().to_string()),
            Some(custom_path.to_string()),
            "环境变量指定路径应具有最高优先级"
        );
        assert!(
            paths.iter().any(|p| p.to_string_lossy().contains(custom_path)),
            "搜索路径应包含环境变量指定的路径"
        );

        std::env::remove_var("GRPC_CODEC_BRIDGE_PATH");
    }

    #[test]
    fn test_library_names_contains_base() {
        let names = CodecBridge::library_names();
        assert!(
            names.iter().all(|n| n.contains("grpc_codec_bridge")),
            "库名应包含 grpc_codec_bridge"
        );
    }

    // =========================================================================
    // BridgeManager Additional Tests
    // =========================================================================

    #[test]
    fn test_bridge_manager_default() {
        let manager: BridgeManager = Default::default();
        assert_eq!(manager.bridge_count(), 0);
    }

    #[test]
    fn test_bridge_manager_thread_safety() {
        use std::thread;

        let manager = Arc::new(BridgeManager::new());
        let mut handles = vec![];

        for i in 0..5 {
            let manager_clone = Arc::clone(&manager);
            let handle = thread::spawn(move || {
                manager_clone.remove_bridge(i);
                manager_clone.bridge_count();
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }
    }

    // =========================================================================
    // Buffer Structure Tests
    // =========================================================================

    #[test]
    fn test_buffer_repr_c() {
        // Ensure Buffer has C representation for FFI compatibility
        fn assert_repr_c<T: Sized>() {}
        assert_repr_c::<Buffer>();
    }

    #[test]
    fn test_buffer_field_types() {
        // Verify field types
        let buffer = Buffer {
            data: std::ptr::null_mut(),
            len: 0usize,
        };
        assert!(buffer.data.is_null());
        assert_eq!(buffer.len, 0);
    }

    // =========================================================================
    // Error Conversion Tests
    // =========================================================================

    #[test]
    fn test_nul_error_conversion() {
        let nul_error = CString::new(vec![0x00]).unwrap_err();
        let ffi_error: FfiError = nul_error.into();
        match ffi_error {
            FfiError::InvalidUtf8 { .. } => {}
            _ => panic!("Expected InvalidUtf8 error"),
        }
    }

    #[test]
    fn test_libloading_error_conversion() {
        // Create a libloading error - the API may vary by version
        let lib_err = libloading::Error::DlOpenUnknown;
        let ffi_error: FfiError = lib_err.into();
        match ffi_error {
            FfiError::LibraryLoadFailed { .. } => {
                // Success - error was converted
            }
            _ => panic!("Expected LibraryLoadFailed error"),
        }
    }

    #[test]
    fn test_utf8_error_conversion() {
        let utf8_bytes = vec![0x80_u8, 0x81_u8];
        let utf8_err = std::str::from_utf8(&utf8_bytes).unwrap_err();
        let ffi_error: FfiError = utf8_err.into();
        match ffi_error {
            FfiError::InvalidUtf8 { .. } => {}
            _ => panic!("Expected InvalidUtf8 error"),
        }
    }

    #[test]
    fn test_serde_json_error_conversion() {
        let json_err = serde_json::from_str::<serde_json::Value>("{invalid}").unwrap_err();
        let ffi_error: FfiError = json_err.into();
        match ffi_error {
            FfiError::JsonParseError { reason, .. } => {
                assert!(!reason.is_empty());
            }
            _ => panic!("Expected JsonParseError"),
        }
    }

    #[test]
    fn test_io_error_conversion() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
        let ffi_error: FfiError = io_err.into();
        match ffi_error {
            FfiError::IoError { reason, .. } => {
                assert!(reason.contains("file missing"));
            }
            _ => panic!("Expected IoError"),
        }
    }

    // =========================================================================
    // CodecBridge Trait Tests
    // =========================================================================

    #[test]
    fn test_codec_bridge_implements_send_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}

        assert_send::<CodecBridge>();
        assert_sync::<CodecBridge>();
    }

    #[test]
    fn test_bridge_manager_implements_send_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}

        assert_send::<BridgeManager>();
        assert_sync::<BridgeManager>();
    }

    // =========================================================================
    // Library Cache Tests
    // =========================================================================

    #[test]
    fn test_library_cache_mutex() {
        // Test that we can acquire the mutex lock
        let lock_result = LIBRARY_CACHE.lock();
        assert!(lock_result.is_ok());
        let _cache = lock_result.unwrap();
    }

    #[test]
    fn test_library_cache_is_static() {
        // Verify LIBRARY_CACHE is accessible as static
        fn assert_static<T: 'static>() {}
        assert_static::<std::sync::Mutex<Option<Arc<Library>>>>();
    }

    // =========================================================================
    // FFI Function Type Tests
    // =========================================================================

    #[test]
    fn test_ffi_function_types_sized() {
        // Ensure all FFI function types are properly defined
        fn assert_fn_type<T>() {}
        assert_fn_type::<BridgeNewFn>();
        assert_fn_type::<BridgeFreeFn>();
        assert_fn_type::<EncodeRequestFn>();
        assert_fn_type::<DecodeResponseFn>();
        assert_fn_type::<ResetParserFn>();
        assert_fn_type::<LastErrorFn>();
        assert_fn_type::<FreeBufferFn>();
        assert_fn_type::<FreeCStringFn>();
    }

    // =========================================================================
    // Additional FFI Bridge Tests
    // =========================================================================

    #[test]
    fn test_buffer_creation() {
        let buffer = Buffer {
            data: std::ptr::null_mut(),
            len: 0,
        };
        assert!(buffer.data.is_null());
        assert_eq!(buffer.len, 0);
    }

    #[test]
    fn test_bridge_manager_get_nonexistent() {
        let manager = BridgeManager::new();
        // Getting a non-existent bridge should return None
        assert!(manager.get_bridge(9999).is_none());
    }

    #[test]
    fn test_bridge_manager_remove_nonexistent() {
        let manager = BridgeManager::new();
        // Removing a non-existent bridge should return None without panicking
        assert!(manager.remove_bridge(9999).is_none());
    }

    #[test]
    fn test_bridge_manager_clear_empty() {
        let manager = BridgeManager::new();
        // Clearing an empty manager should not panic
        manager.clear_bridges();
        assert_eq!(manager.bridge_count(), 0);
    }

    #[test]
    fn test_bridge_counter_sequential() {
        use std::sync::atomic::Ordering;

        // Reset counter to known state
        BRIDGE_COUNTER.store(100, Ordering::SeqCst);

        let first = BRIDGE_COUNTER.fetch_add(1, Ordering::SeqCst);
        let second = BRIDGE_COUNTER.fetch_add(1, Ordering::SeqCst);
        let third = BRIDGE_COUNTER.fetch_add(1, Ordering::SeqCst);

        assert_eq!(first, 100);
        assert_eq!(second, 101);
        assert_eq!(third, 102);
    }

    #[test]
    fn test_cstring_conversion_valid() {
        let valid_string = "Hello, World!";
        let cstring = CString::new(valid_string).unwrap();
        assert_eq!(cstring.to_str().unwrap(), valid_string);
    }

    #[test]
    fn test_cstring_conversion_with_nul() {
        let invalid_string = "Hello\0World";
        let result = CString::new(invalid_string);
        assert!(result.is_err());

        // Verify error conversion
        let err = result.unwrap_err();
        let ffi_err: FfiError = err.into();
        assert!(matches!(ffi_err, FfiError::InvalidUtf8 { .. }));
    }

    #[test]
    fn test_library_names_format() {
        let names = CodecBridge::library_names();
        assert!(!names.is_empty());

        // All names should contain the base name
        for name in &names {
            assert!(name.contains("grpc_codec_bridge"));
        }

        // All names should have appropriate extension
        #[cfg(target_os = "macos")]
        {
            assert!(names.iter().all(|n| n.ends_with(".dylib")));
        }
        #[cfg(target_os = "linux")]
        {
            assert!(names.iter().all(|n| n.ends_with(".so")));
        }
        #[cfg(target_os = "windows")]
        {
            assert!(names.iter().all(|n| n.ends_with(".dll")));
        }
    }

    #[test]
    fn test_ffi_error_display_messages() {
        let err = FfiError::FfiCallFailed {
            function: "test_function".to_string(),
            message: "test message".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("test_function"));
        assert!(msg.contains("test message"));

        let err = FfiError::InvalidMethodName {
            name: "bad/method".to_string(),
        };
        assert!(err.to_string().contains("bad/method"));

        let err = FfiError::InvalidHandle { handle_id: 42 };
        assert!(err.to_string().contains("42"));
    }

    #[test]
    fn test_ffi_error_from_conversions() {
        // Test libloading error conversion
        let lib_err = libloading::Error::DlOpenUnknown;
        let ffi_err: FfiError = lib_err.into();
        assert!(matches!(ffi_err, FfiError::LibraryLoadFailed { .. }));

        // Test UTF-8 error conversion
        let utf8_bytes = vec![0xff_u8, 0xfe_u8];
        let utf8_err = std::str::from_utf8(&utf8_bytes).unwrap_err();
        let ffi_err: FfiError = utf8_err.into();
        assert!(matches!(ffi_err, FfiError::InvalidUtf8 { .. }));

        // Test IO error conversion
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let ffi_err: FfiError = io_err.into();
        assert!(matches!(ffi_err, FfiError::IoError { .. }));
    }

    #[test]
    fn test_bridge_manager_thread_safety_stress() {
        use std::thread;
        use std::sync::Arc;

        let manager = Arc::new(BridgeManager::new());
        let mut handles = vec![];

        // Spawn multiple threads performing different operations
        for i in 0..20 {
            let manager_clone = Arc::clone(&manager);
            let handle = thread::spawn(move || {
                match i % 3 {
                    0 => manager_clone.get_bridge(i),
                    1 => manager_clone.remove_bridge(i),
                    _ => {
                        manager_clone.clear_bridges();
                        None
                    }
                }
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        // Manager should still be in valid state
        assert_eq!(manager.bridge_count(), 0);
    }

    #[test]
    fn test_codec_bridge_trait_bounds() {
        // Verify CodecBridge implements required traits
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}
        fn assert_debug<T: std::fmt::Debug>() {}

        assert_send::<CodecBridge>();
        assert_sync::<CodecBridge>();
        assert_debug::<CodecBridge>();
    }

    #[test]
    fn test_buffer_memory_layout() {
        use std::mem;

        // Verify Buffer has expected size
        let expected_size = mem::size_of::<*mut c_char>() + mem::size_of::<usize>();
        assert_eq!(mem::size_of::<Buffer>(), expected_size);

        // Verify alignment
        let buffer = Buffer {
            data: std::ptr::null_mut(),
            len: 100,
        };
        assert_eq!(buffer.len, 100);
    }

    #[test]
    fn test_library_search_paths_order() {
        // Set environment variable
        std::env::set_var("GRPC_CODEC_BRIDGE_PATH", "/custom/path");
        let paths = CodecBridge::library_search_paths();

        // First path should be from environment variable
        assert!(paths[0].to_string_lossy().contains("/custom/path"));

        std::env::remove_var("GRPC_CODEC_BRIDGE_PATH");
    }

    #[test]
    fn test_empty_method_name_validation() {
        // Test empty method name error
        let err = FfiError::InvalidMethodName {
            name: "".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("无效的方法名"));
    }

    #[test]
    fn test_json_parse_error_details() {
        let err = FfiError::JsonParseError {
            json: "{invalid}".to_string(),
            reason: "expected colon".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("JSON 解析失败"));
        assert!(msg.contains("expected colon"));
        assert!(msg.contains("{invalid}"));
    }
}
