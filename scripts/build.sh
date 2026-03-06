#!/bin/bash
set -e

echo "=== Building gRPC UI ==="

# Configuration
VERSION=${VERSION:-"0.1.0"}
BUILD_DIR="build"
SIDECAR_DIR="sidecar"
SRC_DIR="src"
TAURI_DIR="$SRC_DIR/src-tauri"

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

# Create build directory
mkdir -p "$BUILD_DIR"

# Build FFI library first
echo "Building Go FFI library..."
./scripts/build-ffi.sh --target "$TARGET"

# Verify FFI library exists
echo "Verifying FFI libraries..."
for lib in "$TAURI_DIR"/resources/*grpc_codec_bridge*; do
    if [ -f "$lib" ]; then
        size=$(du -h "$lib" | cut -f1)
        echo "  $(basename "$lib"): $size"
    fi
done

# Install frontend dependencies
echo "Installing frontend dependencies..."
cd "$SRC_DIR"
npm install

# Build frontend
echo "Building frontend..."
npm run build

# Build Tauri app
echo "Building Tauri app..."

if [ -n "$TARGET" ]; then
    # Build for specific target
    case $TARGET in
        darwin-arm64)
            npm run tauri:build -- --target aarch64-apple-darwin
            ;;
        darwin-x64)
            npm run tauri:build -- --target x86_64-apple-darwin
            ;;
        linux-x64)
            npm run tauri:build -- --target x86_64-unknown-linux-gnu
            ;;
        linux-arm64)
            npm run tauri:build -- --target aarch64-unknown-linux-gnu
            ;;
        windows-x64)
            npm run tauri:build -- --target x86_64-pc-windows-msvc
            ;;
        *)
            echo "Unknown target: $TARGET"
            exit 1
            ;;
    esac
else
    # Build for current platform
    npm run tauri:build
fi

cd ..

# Copy build artifacts
echo "Copying build artifacts..."
PLATFORM_DIR=""
if [[ "$OSTYPE" == "darwin"* ]]; then
    PLATFORM_DIR="macos"
    if [ -d "$TAURI_DIR/target/release/bundle/macos" ]; then
        cp -r "$TAURI_DIR/target/release/bundle/macos/"* "$BUILD_DIR/" 2>/dev/null || true
    fi
    if [ -d "$TAURI_DIR/target/release/bundle/dmg" ]; then
        cp -r "$TAURI_DIR/target/release/bundle/dmg/"* "$BUILD_DIR/" 2>/dev/null || true
    fi
elif [[ "$OSTYPE" == "linux-gnu"* ]]; then
    PLATFORM_DIR="linux"
    if [ -d "$TAURI_DIR/target/release/bundle/appimage" ]; then
        cp -r "$TAURI_DIR/target/release/bundle/appimage/"* "$BUILD_DIR/" 2>/dev/null || true
    fi
    if [ -d "$TAURI_DIR/target/release/bundle/deb" ]; then
        cp -r "$TAURI_DIR/target/release/bundle/deb/"* "$BUILD_DIR/" 2>/dev/null || true
    fi
elif [[ "$OSTYPE" == "msys" ]] || [[ "$OSTYPE" == "cygwin" ]] || [[ "$OSTYPE" == "win32" ]]; then
    PLATFORM_DIR="windows"
    if [ -d "$TAURI_DIR/target/release/bundle/msi" ]; then
        cp -r "$TAURI_DIR/target/release/bundle/msi/"* "$BUILD_DIR/" 2>/dev/null || true
    fi
    if [ -d "$TAURI_DIR/target/release/bundle/nsis" ]; then
        cp -r "$TAURI_DIR/target/release/bundle/nsis/"* "$BUILD_DIR/" 2>/dev/null || true
    fi
fi

# Create version info file
cat > "$BUILD_DIR/version.json" << EOF
{
  "version": "$VERSION",
  "platform": "$PLATFORM_DIR",
  "build_time": "$(date -u +"%Y-%m-%dT%H:%M:%SZ")",
  "git_commit": "$(git rev-parse --short HEAD 2>/dev/null || echo 'unknown')"
}
EOF

echo ""
echo "=== Build Complete ==="
echo "Version: $VERSION"
echo "Output directory: $BUILD_DIR/"
echo ""
echo "Build artifacts:"
ls -la "$BUILD_DIR/"
