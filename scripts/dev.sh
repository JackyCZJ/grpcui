#!/bin/bash
set -euo pipefail

echo "=== gRPC UI Development ==="

# 使用脚本自身所在目录推导项目根目录，避免从不同 cwd 启动时出现相对路径偏移。
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
FFI_LIB="${PROJECT_ROOT}/src/src-tauri/resources/libgrpc_codec_bridge"

# 检测平台并设置正确的库文件扩展名
if [[ "$OSTYPE" == "darwin"* ]]; then
    FFI_LIB="${FFI_LIB}.dylib"
    TARGET_TRIPLE="$(uname -m)-apple-darwin"
    if [[ "$TARGET_TRIPLE" == "arm64-apple-darwin" ]]; then
        TARGET_TRIPLE="aarch64-apple-darwin"
    fi
elif [[ "$OSTYPE" == "linux-gnu"* ]]; then
    FFI_LIB="${FFI_LIB}.so"
    TARGET_TRIPLE="$(uname -m)-unknown-linux-gnu"
    if [[ "$TARGET_TRIPLE" == "aarch64-unknown-linux-gnu" ]]; then
        TARGET_TRIPLE="aarch64-unknown-linux-gnu"
    else
        TARGET_TRIPLE="x86_64-unknown-linux-gnu"
    fi
else
    echo "Unsupported platform: $OSTYPE"
    exit 1
fi

# 只要 FFI 源码或依赖声明比库文件更新，就重新构建，避免运行到过期库。
NEEDS_BUILD=false
if [ ! -f "${FFI_LIB}" ]; then
    NEEDS_BUILD=true
elif [ -n "$(find "${PROJECT_ROOT}/sidecar/ffi" -name '*.go' -newer "${FFI_LIB}" -print -quit)" ] \
    || [ "${PROJECT_ROOT}/sidecar/go.mod" -nt "${FFI_LIB}" ] \
    || [ "${PROJECT_ROOT}/sidecar/go.sum" -nt "${FFI_LIB}" ]; then
    NEEDS_BUILD=true
fi

if [ "${NEEDS_BUILD}" = true ]; then
    echo "Building Go FFI library..."
    (
        cd "${PROJECT_ROOT}"
        ./scripts/build-ffi.sh
    )
fi

echo "Using FFI library: ${FFI_LIB}"

# 设置 FFI 库路径供 Rust 加载。
export GRPC_CODEC_BRIDGE_PATH="${FFI_LIB}"

# 将开发数据库放到系统临时目录，避免 SQLite 的 shm/wal 文件写入 src-tauri
# 被 tauri watcher 监听到后反复触发重启。
DEV_DB_DIR="${TMPDIR:-/tmp}/grpcui-dev"
mkdir -p "${DEV_DB_DIR}"
export GRPCUI_DB_PATH="${DEV_DB_DIR}/grpcui.db"

# 显式指定 watcher 忽略文件，兜底规避数据库文件改动引发的重启风暴。
export TAURI_DEV_WATCHER_IGNORE_FILE="${PROJECT_ROOT}/src/.taurignore"

echo "Using SQLite DB: ${GRPCUI_DB_PATH}"
echo "Watcher ignore file: ${TAURI_DEV_WATCHER_IGNORE_FILE}"

echo "Starting frontend + Tauri dev..."
cd "${PROJECT_ROOT}/src"
bun install 2>/dev/null || true
bun run tauri:dev
