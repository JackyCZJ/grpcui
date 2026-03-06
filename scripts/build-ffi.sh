#!/bin/bash
set -e

echo "=== Building Go FFI Library ==="

# Configuration
VERSION=${VERSION:-"0.1.0"}
SIDECAR_DIR="sidecar"
TAURI_DIR="src/src-tauri"
RESOURCES_DIR="$TAURI_DIR/resources"

# Parse arguments
TARGET=""
while [[ $# -gt 0 ]]; do
  case $1 in
    --target)
      TARGET="$2"
      shift 2
      ;;
    --version)
      VERSION="$2"
      shift 2
      ;;
    *)
      echo "Unknown option: $1"
      exit 1
      ;;
  esac
done

# Create resources directory
mkdir -p "$RESOURCES_DIR"

# Function to build FFI library for a specific platform
build_ffi() {
    local os=$1
    local arch=$2
    local output_name=$3

    echo "Building FFI library for $os/$arch..."

    cd "$SIDECAR_DIR"

    # Set build flags
    local ldflags="-s -w -X main.Version=$VERSION"

    # Build as shared library (cgo required)
    if [ "$os" = "darwin" ]; then
        # macOS: build .dylib
        GOOS="$os" GOARCH="$arch" CGO_ENABLED=1 \
            go build -buildmode=c-shared -ldflags="$ldflags" -trimpath \
            -o "../$RESOURCES_DIR/$output_name" ./ffi/
    elif [ "$os" = "linux" ]; then
        # Linux: build .so
        GOOS="$os" GOARCH="$arch" CGO_ENABLED=1 \
            go build -buildmode=c-shared -ldflags="$ldflags" -trimpath \
            -o "../$RESOURCES_DIR/$output_name" ./ffi/
    elif [ "$os" = "windows" ]; then
        # Windows: build .dll
        GOOS="$os" GOARCH="$arch" CGO_ENABLED=1 \
            go build -buildmode=c-shared -ldflags="$ldflags" -trimpath \
            -o "../$RESOURCES_DIR/$output_name" ./ffi/
    fi

    cd ..

    echo "FFI library built: $RESOURCES_DIR/$output_name"
}

# Build for current platform if no target specified
if [ -z "$TARGET" ]; then
    # Detect current platform
    OS=$(uname -s | tr '[:upper:]' '[:lower:]')
    ARCH=$(uname -m)

    case "$OS" in
        darwin)
            case "$ARCH" in
                arm64)
                    build_ffi "darwin" "arm64" "libgrpc_codec_bridge.dylib"
                    ;;
                x86_64)
                    build_ffi "darwin" "amd64" "libgrpc_codec_bridge.dylib"
                    ;;
                *)
                    echo "Unsupported architecture: $ARCH"
                    exit 1
                    ;;
            esac
            ;;
        linux)
            case "$ARCH" in
                x86_64)
                    build_ffi "linux" "amd64" "libgrpc_codec_bridge.so"
                    ;;
                aarch64)
                    build_ffi "linux" "arm64" "libgrpc_codec_bridge.so"
                    ;;
                *)
                    echo "Unsupported architecture: $ARCH"
                    exit 1
                    ;;
            esac
            ;;
        *)
            echo "Unsupported OS: $OS"
            exit 1
            ;;
    esac
else
    # Build for specific target
    case $TARGET in
        darwin-arm64)
            build_ffi "darwin" "arm64" "libgrpc_codec_bridge.dylib"
            ;;
        darwin-x64)
            build_ffi "darwin" "amd64" "libgrpc_codec_bridge.dylib"
            ;;
        linux-x64)
            build_ffi "linux" "amd64" "libgrpc_codec_bridge.so"
            ;;
        linux-arm64)
            build_ffi "linux" "arm64" "libgrpc_codec_bridge.so"
            ;;
        windows-x64)
            build_ffi "windows" "amd64" "grpc_codec_bridge.dll"
            ;;
        *)
            echo "Unknown target: $TARGET"
            exit 1
            ;;
    esac
fi

echo ""
echo "=== FFI Build Complete ==="
echo "Libraries in: $RESOURCES_DIR/"
ls -la "$RESOURCES_DIR/"
