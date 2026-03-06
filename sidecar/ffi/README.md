# gRPC Codec Bridge FFI

Go FFI dynamic library for gRPC codec operations, designed for use by Rust via C ABI.

## Overview

This package provides a C-compatible FFI interface for:
- Loading and parsing Protocol Buffer definitions from files or server reflection
- Encoding JSON payloads to protobuf wire format
- Decoding protobuf wire format to JSON
- Service and method discovery

## Thread Safety

The bridge handle is **NOT** thread-safe. Each thread should create its own handle via `bridge_new()`.
Multiple handles can coexist and operate independently.

## Memory Protocol

- Go allocates all returned memory (strings, buffers)
- Caller must use `free_buffer()` or `free_cstring()` to release memory
- Use `last_error()` to check for errors when functions return NULL

## Build

### macOS
```bash
go build -buildmode=c-shared -o libgrpc_codec_bridge.dylib *.go
```

### Linux
```bash
go build -buildmode=c-shared -o libgrpc_codec_bridge.so *.go
```

### Windows
```bash
go build -buildmode=c-shared -o grpc_codec_bridge.dll *.go
```

## C ABI Interface

### Types

```c
// Opaque handle type
typedef uintptr_t BridgeHandle;

// Buffer for binary data
typedef struct {
    char* data;
    size_t len;
} Buffer;
```

### Lifecycle Functions

#### `bridge_new()`
Creates a new bridge handle.

**Returns:**
- `BridgeHandle` - Opaque handle to the bridge instance

**Example:**
```c
BridgeHandle handle = bridge_new();
if (handle == 0) {
    // Handle creation failed
}
```

#### `bridge_free(handle)`
Frees a bridge handle and associated resources.

**Parameters:**
- `handle` - Bridge handle returned by `bridge_new()`

**Example:**
```c
bridge_free(handle);
```

### Proto Loading Functions

#### `load_proto_files(handle, proto_paths_json, import_paths_json)`
Loads proto definitions from .proto files.

**Parameters:**
- `handle` - Bridge handle
- `proto_paths_json` - JSON array of proto file paths (e.g., `["/path/to/file.proto"]`)
- `import_paths_json` - JSON array of import paths (can be NULL)

**Returns:**
- `0` on success
- `-1` on error (check `last_error()`)

**Example:**
```c
const char* proto_paths = "[\"/path/to/service.proto\"]";
const char* import_paths = "[\"/path/to/imports\"]";
int result = load_proto_files(handle, proto_paths, import_paths);
if (result != 0) {
    char* err = last_error();
    printf("Error: %s\n", err);
    free_cstring(err);
}
```

#### `load_reflection(handle, target_addr, tls_config_json)`
Loads proto definitions via gRPC server reflection.

**Parameters:**
- `handle` - Bridge handle
- `target_addr` - Target server address (e.g., `localhost:50051`)
- `tls_config_json` - TLS configuration JSON (can be NULL for default TLS)

**TLS Config JSON:**
```json
{
    "insecure": false,
    "cert_path": "/path/to/cert.pem",
    "key_path": "/path/to/key.pem",
    "ca_path": "/path/to/ca.pem"
}
```

**Returns:**
- `0` on success
- `-1` on error (check `last_error()`)

**Example:**
```c
const char* tls_config = "{\"insecure\": true}";
int result = load_reflection(handle, "localhost:50051", tls_config);
```

### Service Discovery Functions

#### `list_services(handle)`
Returns a JSON array of all available services.

**Parameters:**
- `handle` - Bridge handle

**Returns:**
- JSON string on success (caller must free with `free_cstring()`)
- `NULL` on error (check `last_error()`)

**Response Format:**
```json
{
    "services": [
        {
            "name": "TestService",
            "full_name": "test.TestService",
            "methods": [...]
        }
    ]
}
```

**Example:**
```c
char* services = list_services(handle);
if (services) {
    printf("Services: %s\n", services);
    free_cstring(services);
}
```

#### `list_methods(handle, service_name)`
Returns a JSON array of methods for a specific service.

**Parameters:**
- `handle` - Bridge handle
- `service_name` - Full service name (e.g., `test.TestService`)

**Returns:**
- JSON string on success (caller must free with `free_cstring()`)
- `NULL` on error (check `last_error()`)

**Response Format:**
```json
{
    "methods": [
        {
            "name": "TestMethod",
            "full_name": "test.TestService.TestMethod",
            "input_type": "test.TestRequest",
            "output_type": "test.TestResponse",
            "type": "unary",
            "client_streaming": false,
            "server_streaming": false
        }
    ]
}
```

**Method Types:**
- `unary` - Standard request/response
- `client_stream` - Client streaming
- `server_stream` - Server streaming
- `bidi_stream` - Bidirectional streaming

### Encoding/Decoding Functions

#### `encode_request_json_to_wire(handle, method_name, json_payload)`
Encodes a JSON payload to protobuf wire format.

**Parameters:**
- `handle` - Bridge handle
- `method_name` - Method name (format: `service/method` or `service.method`)
- `json_payload` - JSON string to encode

**Returns:**
- `Buffer*` on success (caller must free with `free_buffer()`)
- `NULL` on error (check `last_error()`)

**Example:**
```c
const char* json = "{\"name\": \"test\", \"id\": 123}";
Buffer* buf = encode_request_json_to_wire(handle, "test.TestService/TestMethod", json);
if (buf) {
    // Use buf->data and buf->len
    send_over_network(buf->data, buf->len);
    free_buffer(buf);
}
```

#### `decode_response_wire_to_json(handle, method_name, wire_data, wire_len)`
Decodes protobuf wire format to JSON.

**Parameters:**
- `handle` - Bridge handle
- `method_name` - Method name (format: `service/method` or `service.method`)
- `wire_data` - Pointer to wire format bytes
- `wire_len` - Length of wire data

**Returns:**
- JSON string on success (caller must free with `free_cstring()`)
- `NULL` on error (check `last_error()`)

**Example:**
```c
char* json = decode_response_wire_to_json(handle, "test.TestService/TestMethod",
                                          response_data, response_len);
if (json) {
    printf("Response: %s\n", json);
    free_cstring(json);
}
```

### Error Handling Functions

#### `last_error()`
Returns the last error message.

**Returns:**
- Error string on success (caller must free with `free_cstring()`)
- `NULL` if no error

**Example:**
```c
char* err = last_error();
if (err) {
    printf("Error: %s\n", err);
    free_cstring(err);
}
```

### Memory Management Functions

#### `free_buffer(buffer)`
Frees a buffer allocated by Go.

**Parameters:**
- `buffer` - Buffer pointer returned by `encode_request_json_to_wire()`

#### `free_cstring(string)`
Frees a C string allocated by Go.

**Parameters:**
- `string` - String pointer returned by functions that return `char*`

## Complete Example (C)

```c
#include <stdio.h>
#include <stdlib.h>
#include "libgrpc_codec_bridge.h"

int main() {
    // Create bridge
    BridgeHandle handle = bridge_new();
    if (handle == 0) {
        fprintf(stderr, "Failed to create bridge\n");
        return 1;
    }

    // Load proto files
    const char* proto_paths = "[\"/path/to/service.proto\"]";
    if (load_proto_files(handle, proto_paths, NULL) != 0) {
        char* err = last_error();
        fprintf(stderr, "Failed to load proto: %s\n", err);
        free_cstring(err);
        bridge_free(handle);
        return 1;
    }

    // List services
    char* services = list_services(handle);
    if (services) {
        printf("Services: %s\n", services);
        free_cstring(services);
    }

    // Encode request
    const char* json = "{\"name\": \"test\"}";
    Buffer* buf = encode_request_json_to_wire(handle, "my.Service/Method", json);
    if (buf) {
        printf("Encoded %zu bytes\n", buf->len);
        // Send buf->data over network...
        free_buffer(buf);
    }

    // Cleanup
    bridge_free(handle);
    return 0;
}
```

## Error Handling Pattern

All functions follow this error handling pattern:
1. On success: Return valid pointer or 0
2. On error: Return NULL or -1, set error message accessible via `last_error()`

Always check return values and call `last_error()` when a function indicates failure.

## Dependencies

- Go 1.23+
- jhump/protoreflect
- google.golang.org/grpc
- google.golang.org/protobuf
