//go:build cgo

package main

/*
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

// Buffer represents a byte buffer returned to C
typedef struct {
    char* data;
    size_t len;
} Buffer;
*/
import "C"

import (
	"crypto/tls"
	"encoding/json"
	"fmt"
	"path/filepath"
	"strings"
	"unsafe"

	localproto "github.com/jacky/grpcui/sidecar/internal/proto"
	"google.golang.org/grpc"
	"google.golang.org/grpc/connectivity"
	"google.golang.org/grpc/credentials"
	"google.golang.org/grpc/credentials/insecure"
	"google.golang.org/protobuf/encoding/protojson"
	"google.golang.org/protobuf/proto"
)

// closeConnQuietly 会在连接非空时尝试关闭连接。
//
// 关闭失败通常只影响资源回收，不影响本桥接层对外语义；这里显式读取并处理错误，
// 主要目的是满足 errcheck，同时保持原有行为不变（不向调用方返回额外错误）。
func closeConnQuietly(conn *grpc.ClientConn) {
	if conn == nil {
		return
	}

	if err := conn.Close(); err != nil {
		// 这里故意忽略 close 错误，避免改变现有 API 行为。
		_ = err
	}
}

// isConnectionHealthy checks if a gRPC connection is in a healthy state
func isConnectionHealthy(conn *grpc.ClientConn) bool {
	if conn == nil {
		return false
	}
	state := conn.GetState()
	// IDLE and READY are considered healthy states
	return state == connectivity.Idle || state == connectivity.Ready
}

// parseMethodName 负责把方法名统一解析成 service/method。
// 支持三种输入：
// 1) service/method
// 2) /service/method
// 3) service.method
func parseMethodName(methodName string) (string, string, error) {
	normalized := strings.TrimSpace(methodName)
	normalized = strings.TrimPrefix(normalized, "/")
	if normalized == "" {
		return "", "", fmt.Errorf("invalid method name format: %s", methodName)
	}

	if parts := strings.SplitN(normalized, "/", 2); len(parts) == 2 {
		if parts[0] != "" && parts[1] != "" {
			return parts[0], parts[1], nil
		}
	}

	dot := strings.LastIndex(normalized, ".")
	if dot > 0 && dot+1 < len(normalized) {
		return normalized[:dot], normalized[dot+1:], nil
	}

	return "", "", fmt.Errorf("invalid method name format: %s", methodName)
}

// newCBufferFromBytes 把 Go 字节切片安全复制到 C.Buffer。
//
// 关键点：
// 1) 空切片也会分配 1 字节占位内存，避免对 `bytes[0]` 取址导致 panic；
// 2) 返回的 data/len 与实际 payload 对齐，读取方依旧只会按 len 处理；
// 3) 内存统一由 free_buffer 回收，保证跨语言释放路径一致。
func newCBufferFromBytes(payload []byte) (*C.Buffer, error) {
	buf := (*C.Buffer)(C.malloc(C.size_t(unsafe.Sizeof(C.Buffer{}))))
	if buf == nil {
		return nil, fmt.Errorf("failed to allocate buffer")
	}

	allocSize := len(payload)
	if allocSize == 0 {
		allocSize = 1
	}

	buf.data = (*C.char)(C.malloc(C.size_t(allocSize)))
	if buf.data == nil {
		C.free(unsafe.Pointer(buf))
		return nil, fmt.Errorf("failed to allocate buffer data")
	}

	if len(payload) > 0 {
		C.memcpy(unsafe.Pointer(buf.data), unsafe.Pointer(&payload[0]), C.size_t(len(payload)))
	}

	buf.len = C.size_t(len(payload))
	return buf, nil
}

// bridge_new creates a new bridge handle
//
//export bridge_new
func bridge_new() C.uintptr_t {
	bridge := &Bridge{
		parser: localproto.NewParser(),
	}
	handle := store.add(bridge)
	return C.uintptr_t(handle)
}

// bridge_free frees a bridge handle
//
//export bridge_free
func bridge_free(handle C.uintptr_t) {
	id := uintptr(handle)
	bridge, ok := store.get(id)
	if !ok {
		return
	}

	// 释放桥接句柄时统一重置内部状态，确保连接与描述符缓存都被彻底清理。
	resetBridgeState(bridge)

	store.remove(id)
}

// resetBridgeState 用于把单个 bridge 恢复到“全新未加载”状态。
//
// 该函数会在持锁状态下完成三件事：
// 1) 关闭并清空当前 gRPC 连接，避免后续仍复用旧连接；
// 2) 重建 parser 实例，保证服务/方法描述符不会跨项目残留；
// 3) 保持 bridge 句柄不变，避免影响上层 Rust 持有的 handle。
//
// 选择“重建 parser”而不是“手动清空字段”，可确保未来 parser 内部结构扩展时
// 也能自动继承重置语义，减少遗漏风险。
func resetBridgeState(bridge *Bridge) {
	bridge.mu.Lock()
	defer bridge.mu.Unlock()

	closeConnQuietly(bridge.conn)
	bridge.conn = nil
	bridge.parser = localproto.NewParser()
}

// reset_parser 重置桥接实例的运行态缓存与连接。
//
// 该接口供上层在“切换项目/断开连接”时显式调用，保证下一次加载 proto 前
// 后端状态完全干净，不会混入上一个项目的服务描述。
//
//export reset_parser
func reset_parser(handle C.uintptr_t) C.int {
	bridge, ok := store.get(uintptr(handle))
	if !ok {
		setLastError(fmt.Errorf("invalid bridge handle"))
		return -1
	}

	resetBridgeState(bridge)
	setLastError(nil)
	return 0
}

// load_proto_files loads proto files from disk
//
//export load_proto_files
func load_proto_files(handle C.uintptr_t, protoPathsJSON *C.char, importPathsJSON *C.char) C.int {
	bridge, ok := store.get(uintptr(handle))
	if !ok {
		setLastError(fmt.Errorf("invalid bridge handle"))
		return -1
	}

	var protoPaths []string
	var importPaths []string

	if protoPathsJSON == nil {
		setLastError(fmt.Errorf("proto_paths is required"))
		return -1
	}

	if err := json.Unmarshal([]byte(C.GoString(protoPathsJSON)), &protoPaths); err != nil {
		setLastError(fmt.Errorf("failed to parse proto_paths: %w", err))
		return -1
	}

	if importPathsJSON != nil {
		if err := json.Unmarshal([]byte(C.GoString(importPathsJSON)), &importPaths); err != nil {
			setLastError(fmt.Errorf("failed to parse import_paths: %w", err))
			return -1
		}
	}

	// Add import paths from proto file directories
	for _, path := range protoPaths {
		dir := filepath.Dir(path)
		if dir != "" && dir != "." {
			importPaths = append(importPaths, dir)
		}
	}

	bridge.mu.Lock()
	defer bridge.mu.Unlock()

	if err := bridge.parser.LoadFromFileWithImports(protoPaths, importPaths); err != nil {
		setLastError(err)
		return -1
	}

	setLastError(nil)
	return 0
}

// load_reflection loads proto definitions via server reflection
//
//export load_reflection
func load_reflection(handle C.uintptr_t, targetAddr *C.char, tlsConfigJSON *C.char) C.int {
	bridge, ok := store.get(uintptr(handle))
	if !ok {
		setLastError(fmt.Errorf("invalid bridge handle"))
		return -1
	}

	if targetAddr == nil {
		setLastError(fmt.Errorf("target_addr is required"))
		return -1
	}

	addr := C.GoString(targetAddr)

	// Parse TLS config
	var tlsConfig TLSConfig
	if tlsConfigJSON != nil {
		if err := json.Unmarshal([]byte(C.GoString(tlsConfigJSON)), &tlsConfig); err != nil {
			setLastError(fmt.Errorf("failed to parse tls_config: %w", err))
			return -1
		}
	}

	// Check if we already have a healthy connection
	bridge.mu.Lock()
	if bridge.conn != nil {
		if isConnectionHealthy(bridge.conn) {
			bridge.mu.Unlock()
			setLastError(nil)
			return 0
		}
		// Close unhealthy connection
		closeConnQuietly(bridge.conn)
		bridge.conn = nil
	}
	bridge.mu.Unlock()

	// Build dial options
	dialOpts := []grpc.DialOption{}

	if tlsConfig.Insecure {
		dialOpts = append(dialOpts, grpc.WithTransportCredentials(insecure.NewCredentials()))
	} else if tlsConfig.CertPath != "" {
		cert, err := tls.LoadX509KeyPair(tlsConfig.CertPath, tlsConfig.KeyPath)
		if err != nil {
			setLastError(fmt.Errorf("failed to load TLS cert: %w", err))
			return -1
		}
		config := &tls.Config{Certificates: []tls.Certificate{cert}}
		dialOpts = append(dialOpts, grpc.WithTransportCredentials(credentials.NewTLS(config)))
	} else {
		dialOpts = append(dialOpts, grpc.WithTransportCredentials(credentials.NewTLS(&tls.Config{})))
	}

	// Create new connection outside of lock to avoid blocking
	conn, err := grpc.NewClient(addr, dialOpts...)
	if err != nil {
		setLastError(fmt.Errorf("failed to connect: %w", err))
		return -1
	}

	// Test connection
	state := conn.GetState()
	if state == connectivity.TransientFailure {
		closeConnQuietly(conn)
		setLastError(fmt.Errorf("connection failed"))
		return -1
	}

	// Re-acquire lock to set the new connection
	bridge.mu.Lock()
	defer bridge.mu.Unlock()

	// Double-check: another goroutine may have set the connection while we were waiting
	if bridge.conn != nil {
		if isConnectionHealthy(bridge.conn) {
			// Another goroutine succeeded, close our connection and return success
			closeConnQuietly(conn)
			setLastError(nil)
			return 0
		}
		// Existing connection is unhealthy, replace it
		closeConnQuietly(bridge.conn)
	}

	bridge.conn = conn

	if err := bridge.parser.LoadFromReflection(conn); err != nil {
		closeConnQuietly(conn)
		bridge.conn = nil
		setLastError(err)
		return -1
	}

	setLastError(nil)
	return 0
}

// list_services returns a JSON array of all services
//
//export list_services
func list_services(handle C.uintptr_t) *C.char {
	bridge, ok := store.get(uintptr(handle))
	if !ok {
		setLastError(fmt.Errorf("invalid bridge handle"))
		return nil
	}

	bridge.mu.RLock()
	defer bridge.mu.RUnlock()

	services := bridge.parser.GetServices()
	response := ServiceListResponse{Services: services}

	result := toJSONString(response)
	return C.CString(result)
}

// list_methods returns a JSON array of methods for a service
//
//export list_methods
func list_methods(handle C.uintptr_t, serviceName *C.char) *C.char {
	bridge, ok := store.get(uintptr(handle))
	if !ok {
		setLastError(fmt.Errorf("invalid bridge handle"))
		return nil
	}

	if serviceName == nil {
		setLastError(fmt.Errorf("service_name is required"))
		return nil
	}

	bridge.mu.RLock()
	defer bridge.mu.RUnlock()

	methods, err := bridge.parser.GetMethods(C.GoString(serviceName))
	if err != nil {
		setLastError(err)
		return nil
	}

	response := MethodListResponse{Methods: methods}
	result := toJSONString(response)
	return C.CString(result)
}

// get_method_input_schema returns request message schema for a method.
//
//export get_method_input_schema
func get_method_input_schema(handle C.uintptr_t, serviceName *C.char, methodName *C.char) *C.char {
	bridge, ok := store.get(uintptr(handle))
	if !ok {
		setLastError(fmt.Errorf("invalid bridge handle"))
		return nil
	}

	if serviceName == nil || methodName == nil {
		setLastError(fmt.Errorf("service_name and method_name are required"))
		return nil
	}

	bridge.mu.RLock()
	defer bridge.mu.RUnlock()

	schema, err := bridge.parser.GetMethodInputSchema(C.GoString(serviceName), C.GoString(methodName))
	if err != nil {
		setLastError(err)
		return nil
	}

	result := toJSONString(schema)
	setLastError(nil)
	return C.CString(result)
}

// encode_request_json_to_wire encodes JSON payload to wire format
//
//export encode_request_json_to_wire
func encode_request_json_to_wire(handle C.uintptr_t, methodName *C.char, jsonPayload *C.char) *C.Buffer {
	bridge, ok := store.get(uintptr(handle))
	if !ok {
		setLastError(fmt.Errorf("invalid bridge handle"))
		return nil
	}

	if methodName == nil || jsonPayload == nil {
		setLastError(fmt.Errorf("method_name and json_payload are required"))
		return nil
	}

	methodStr := C.GoString(methodName)
	jsonStr := C.GoString(jsonPayload)

	bridge.mu.RLock()
	defer bridge.mu.RUnlock()

	serviceName, methodNameOnly, err := parseMethodName(methodStr)
	if err != nil {
		setLastError(err)
		return nil
	}

	// Get input type descriptor
	inputDesc, err := bridge.parser.GetInputType(serviceName, methodNameOnly)
	if err != nil {
		setLastError(fmt.Errorf("failed to get input type: %w", err))
		return nil
	}

	// Create message and parse JSON
	msg := localproto.CreateMessageFromDesc(inputDesc)
	if err := protojson.Unmarshal([]byte(jsonStr), msg); err != nil {
		setLastError(fmt.Errorf("failed to unmarshal JSON: %w", err))
		return nil
	}

	// Marshal to wire format
	wireData, err := proto.Marshal(msg)
	if err != nil {
		setLastError(fmt.Errorf("failed to marshal to wire format: %w", err))
		return nil
	}

	buf, err := newCBufferFromBytes(wireData)
	if err != nil {
		setLastError(err)
		return nil
	}

	setLastError(nil)
	return buf
}

// decode_response_wire_to_json decodes wire format bytes to JSON
//
//export decode_response_wire_to_json
func decode_response_wire_to_json(handle C.uintptr_t, methodName *C.char, wireData *C.char, wireLen C.size_t) *C.char {
	bridge, ok := store.get(uintptr(handle))
	if !ok {
		setLastError(fmt.Errorf("invalid bridge handle"))
		return nil
	}

	if methodName == nil || wireData == nil {
		setLastError(fmt.Errorf("method_name and wire_data are required"))
		return nil
	}

	methodStr := C.GoString(methodName)
	wireBytes := C.GoBytes(unsafe.Pointer(wireData), C.int(wireLen))

	bridge.mu.RLock()
	defer bridge.mu.RUnlock()

	serviceName, methodNameOnly, err := parseMethodName(methodStr)
	if err != nil {
		setLastError(err)
		return nil
	}

	// Get output type descriptor
	outputDesc, err := bridge.parser.GetOutputType(serviceName, methodNameOnly)
	if err != nil {
		setLastError(fmt.Errorf("failed to get output type: %w", err))
		return nil
	}

	// Create message and unmarshal from wire format
	msg := localproto.CreateMessageFromDesc(outputDesc)
	if err := proto.Unmarshal(wireBytes, msg); err != nil {
		setLastError(fmt.Errorf("failed to unmarshal wire data: %w", err))
		return nil
	}

	// Marshal to JSON
	jsonData, err := protojson.Marshal(msg)
	if err != nil {
		setLastError(fmt.Errorf("failed to marshal to JSON: %w", err))
		return nil
	}

	setLastError(nil)
	return C.CString(string(jsonData))
}

// last_error returns the last error message
//
//export last_error
func last_error() *C.char {
	err := getLastError()
	if err == "" {
		return nil
	}
	return C.CString(err)
}

// free_buffer frees a buffer allocated by Go
//
//export free_buffer
func free_buffer(buf unsafe.Pointer) {
	if buf == nil {
		return
	}

	cBuf := (*C.Buffer)(buf)
	if cBuf.data != nil {
		C.free(unsafe.Pointer(cBuf.data))
		cBuf.data = nil
	}

	C.free(buf)
}

// free_cstring frees a C string allocated by Go
//
//export free_cstring
func free_cstring(s *C.char) {
	if s != nil {
		C.free(unsafe.Pointer(s))
	}
}

func main() {}
