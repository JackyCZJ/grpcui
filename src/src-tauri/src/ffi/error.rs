//! FFI 错误处理模块
//!
//! 本模块定义了与 Go FFI 动态库交互过程中可能发生的所有错误类型，
//! 并提供与标准 Error trait 的集成。

#![allow(dead_code)]
use std::fmt;
use std::path::PathBuf;

/// FFI 操作的结果类型别名
pub type Result<T> = std::result::Result<T, FfiError>;

/// FFI 错误类型枚举
///
/// 涵盖了从动态库加载到函数调用的全生命周期可能遇到的错误。
#[derive(Debug, Clone)]
pub enum FfiError {
    /// 动态库未找到
    ///
    /// 可能原因：
    /// - 库文件不存在于预期路径
    /// - 环境变量 `GRPC_CODEC_BRIDGE_PATH` 指向无效路径
    /// - 应用资源目录中未找到捆绑的库
    LibraryNotFound {
        /// 尝试查找的路径列表
        paths: Vec<PathBuf>,
    },

    /// 动态库加载失败
    ///
    /// 库文件存在但无法加载，可能原因：
    /// - 架构不匹配（如 ARM64 系统加载 x86_64 库）
    /// - 依赖库缺失
    /// - 文件损坏
    LibraryLoadFailed {
        /// 库文件路径
        path: PathBuf,
        /// 底层错误信息
        reason: String,
    },

    /// FFI 符号未找到
    ///
    /// 动态库中不存在指定的函数符号，可能原因：
    /// - 库版本不匹配
    /// - 函数名拼写错误
    SymbolNotFound {
        /// 符号名称
        name: String,
    },

    /// FFI 调用失败
    ///
    /// Go 侧函数执行返回错误，通过 `last_error()` 获取详细错误信息。
    FfiCallFailed {
        /// 调用的函数名
        function: String,
        /// 错误信息
        message: String,
    },

    /// 无效的 UTF-8 序列
    ///
    /// Go 返回的字符串包含无效的 UTF-8 字节序列。
    InvalidUtf8 {
        /// 原始字节数据（用于调试）
        bytes: Vec<u8>,
    },

    /// JSON 解析错误
    ///
    /// 无法解析 Go 返回的 JSON 数据。
    JsonParseError {
        /// 原始 JSON 字符串
        json: String,
        /// 解析错误信息
        reason: String,
    },

    /// 空指针错误
    ///
    /// FFI 调用返回了意外的空指针。
    NullPointer {
        /// 发生空指针的上下文
        context: String,
    },

    /// 无效的方法名
    ///
    /// 方法名格式不符合 `ServiceName/MethodName` 规范。
    InvalidMethodName {
        /// 无效的方法名
        name: String,
    },

    /// 桥接句柄无效
    ///
    /// 尝试使用已释放或无效的桥接句柄。
    InvalidHandle {
        /// 句柄 ID
        handle_id: usize,
    },

    /// IO 错误
    ///
    /// 文件系统操作失败。
    IoError {
        /// 操作上下文
        context: String,
        /// 底层错误信息
        reason: String,
    },
}

impl fmt::Display for FfiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FfiError::LibraryNotFound { paths } => {
                write!(
                    f,
                    "FFI 库未找到，已尝试路径: {}",
                    paths.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", ")
                )
            }
            FfiError::LibraryLoadFailed { path, reason } => {
                write!(f, "无法加载 FFI 库 '{}': {}", path.display(), reason)
            }
            FfiError::SymbolNotFound { name } => {
                write!(f, "FFI 符号 '{}' 未在动态库中找到", name)
            }
            FfiError::FfiCallFailed { function, message } => {
                write!(f, "FFI 调用 '{}' 失败: {}", function, message)
            }
            FfiError::InvalidUtf8 { .. } => {
                write!(f, "FFI 返回了无效的 UTF-8 数据")
            }
            FfiError::JsonParseError { json, reason } => {
                write!(f, "JSON 解析失败: {}，原始数据: {}", reason, json)
            }
            FfiError::NullPointer { context } => {
                write!(f, "在 '{}' 中遇到空指针", context)
            }
            FfiError::InvalidMethodName { name } => {
                write!(f, "无效的方法名 '{}', 期望格式: ServiceName/MethodName", name)
            }
            FfiError::InvalidHandle { handle_id } => {
                write!(f, "无效的桥接句柄 ID: {}", handle_id)
            }
            FfiError::IoError { context, reason } => {
                write!(f, "IO 错误 ({}): {}", context, reason)
            }
        }
    }
}

impl std::error::Error for FfiError {}

// 从 libloading::Error 转换
impl From<libloading::Error> for FfiError {
    fn from(err: libloading::Error) -> Self {
        match err {
            libloading::Error::DlOpen { desc } | libloading::Error::DlSym { desc } => {
                FfiError::LibraryLoadFailed {
                    path: PathBuf::from("unknown"),
                    reason: format!("{:?}", desc),
                }
            }
            _ => FfiError::LibraryLoadFailed {
                path: PathBuf::from("unknown"),
                reason: err.to_string(),
            },
        }
    }
}

// 从 std::str::Utf8Error 转换
impl From<std::str::Utf8Error> for FfiError {
    fn from(_err: std::str::Utf8Error) -> Self {
        FfiError::InvalidUtf8 { bytes: Vec::new() }
    }
}

// 从 serde_json::Error 转换
impl From<serde_json::Error> for FfiError {
    fn from(err: serde_json::Error) -> Self {
        FfiError::JsonParseError {
            json: String::new(),
            reason: err.to_string(),
        }
    }
}

// 从 std::io::Error 转换
impl From<std::io::Error> for FfiError {
    fn from(err: std::io::Error) -> Self {
        FfiError::IoError {
            context: "IO 操作".to_string(),
            reason: err.to_string(),
        }
    }
}

// 从 std::ffi::NulError 转换（CString::new 错误）
impl From<std::ffi::NulError> for FfiError {
    fn from(_err: std::ffi::NulError) -> Self {
        FfiError::InvalidUtf8 {
            bytes: Vec::new(),
        }
    }
}

/// 用于 FFI 返回码的辅助类型
///
/// Go FFI 函数通常返回 0 表示成功，非零表示失败。
pub type FfiResultCode = i32;

/// FFI 成功返回码
pub const FFI_OK: FfiResultCode = 0;

/// FFI 通用错误返回码
pub const FFI_ERROR: FfiResultCode = -1;

/// 检查 FFI 返回码并转换为 Result
///
/// # 参数
/// - `code`: FFI 函数返回的错误码
/// - `function`: 调用的函数名（用于错误信息）
/// - `get_error`: 获取错误信息的闭包
///
/// # 返回
/// - `Ok(())` 如果 `code == FFI_OK`
/// - `Err(FfiError::FfiCallFailed)` 否则
pub fn check_ffi_result<F>(code: FfiResultCode, function: &str, get_error: F) -> Result<()>
where
    F: FnOnce() -> String,
{
    if code == FFI_OK {
        Ok(())
    } else {
        Err(FfiError::FfiCallFailed {
            function: function.to_string(),
            message: get_error(),
        })
    }
}
