//go:build cgo

package main

import (
	"encoding/json"
	"os"
	"path/filepath"
	"testing"
	"unsafe"

	"github.com/jacky/grpcui/sidecar/internal/proto"
)

// chdirForTest 将当前工作目录切换到 dir，并在测试结束时自动恢复。
//
// 该辅助函数集中处理目录切换的错误检查，避免测试代码里散落未检查的 os.Chdir 调用。
// 使用 t.Cleanup 可以保证即使测试中途失败，也会尽力恢复原目录，减少测试之间的相互影响。
func chdirForTest(t *testing.T, dir string) {
	t.Helper()

	origDir, err := os.Getwd()
	if err != nil {
		t.Fatalf("failed to get current dir: %v", err)
	}

	if err := os.Chdir(dir); err != nil {
		t.Fatalf("failed to chdir to %s: %v", dir, err)
	}

	t.Cleanup(func() {
		if err := os.Chdir(origDir); err != nil {
			t.Fatalf("failed to restore working dir to %s: %v", origDir, err)
		}
	})
}

func TestBridgeStore(t *testing.T) {
	// Test store operations directly
	bridge := &Bridge{
		parser: proto.NewParser(),
	}

	// Test add
	handle := store.add(bridge)
	if handle == 0 {
		t.Fatal("store.add returned 0 handle")
	}

	// Test get
	retrieved, ok := store.get(handle)
	if !ok {
		t.Fatal("store.get failed to retrieve bridge")
	}
	if retrieved == nil {
		t.Fatal("retrieved bridge is nil")
	}
	if retrieved.parser == nil {
		t.Fatal("retrieved bridge parser is nil")
	}

	// Test remove
	store.remove(handle)
	_, ok = store.get(handle)
	if ok {
		t.Fatal("store.remove failed to remove bridge")
	}
}

func TestMultipleBridges(t *testing.T) {
	// Create multiple bridges
	handles := make([]uintptr, 5)
	for i := 0; i < 5; i++ {
		bridge := &Bridge{parser: proto.NewParser()}
		handles[i] = store.add(bridge)
		if handles[i] == 0 {
			t.Fatalf("store.add returned 0 handle at index %d", i)
		}
	}

	// Verify all handles are unique
	seen := make(map[uintptr]bool)
	for _, h := range handles {
		if seen[h] {
			t.Fatal("duplicate bridge handle")
		}
		seen[h] = true
	}

	// Free all bridges
	for _, h := range handles {
		store.remove(h)
	}
}

func TestLoadProtoFiles(t *testing.T) {
	// Create a temporary proto file for testing
	tmpDir := t.TempDir()
	protoContent := `
syntax = "proto3";
package test;

message TestRequest {
    string name = 1;
    int32 id = 2;
}

message TestResponse {
    string message = 1;
    bool success = 2;
}

service TestService {
    rpc TestMethod (TestRequest) returns (TestResponse);
}
`
	protoPath := filepath.Join(tmpDir, "test.proto")
	if err := os.WriteFile(protoPath, []byte(protoContent), 0644); err != nil {
		t.Fatalf("failed to write proto file: %v", err)
	}

	bridge := &Bridge{parser: proto.NewParser()}
	handle := store.add(bridge)
	defer store.remove(handle)

	// Change to temp directory and use relative path
	chdirForTest(t, tmpDir)

	// Test loading proto files directly via parser with relative path
	if err := bridge.parser.LoadFromFile("test.proto"); err != nil {
		t.Fatalf("LoadFromFile failed: %v", err)
	}

	// Verify services are loaded
	services := bridge.parser.GetServices()
	if len(services) == 0 {
		t.Fatal("no services loaded")
	}

	found := false
	for _, svc := range services {
		if svc.FullName == "test.TestService" {
			found = true
			break
		}
	}
	if !found {
		t.Fatal("TestService not found in loaded services")
	}
}

func TestResetBridgeStateClearsParserCache(t *testing.T) {
	tmpDir := t.TempDir()
	protoContent := `
syntax = "proto3";
package test;

message Req {}
message Resp {}

service TestService {
    rpc Ping (Req) returns (Resp);
}
`
	if err := os.WriteFile(filepath.Join(tmpDir, "test.proto"), []byte(protoContent), 0644); err != nil {
		t.Fatalf("failed to write proto file: %v", err)
	}

	chdirForTest(t, tmpDir)

	bridge := &Bridge{parser: proto.NewParser()}
	if err := bridge.parser.LoadFromFile("test.proto"); err != nil {
		t.Fatalf("LoadFromFile failed: %v", err)
	}

	if len(bridge.parser.GetServices()) == 0 {
		t.Fatal("expected services before reset, got none")
	}

	// resetBridgeState 应把 parser 与连接恢复为全新状态，避免跨项目残留。
	resetBridgeState(bridge)

	if bridge.conn != nil {
		t.Fatal("expected grpc conn to be nil after reset")
	}
	if len(bridge.parser.GetServices()) != 0 {
		t.Fatal("expected no services after reset")
	}
}

func TestListServices(t *testing.T) {
	// Create a temporary proto file
	tmpDir := t.TempDir()
	protoContent := `
syntax = "proto3";
package test;

message Req {}
message Resp {}

service Service1 {
    rpc Method1 (Req) returns (Resp);
}

service Service2 {
    rpc Method2 (Req) returns (Resp);
}
`
	if err := os.WriteFile(filepath.Join(tmpDir, "test.proto"), []byte(protoContent), 0644); err != nil {
		t.Fatalf("failed to write proto file: %v", err)
	}

	// Change to temp directory
	chdirForTest(t, tmpDir)

	bridge := &Bridge{parser: proto.NewParser()}
	if err := bridge.parser.LoadFromFile("test.proto"); err != nil {
		t.Fatalf("LoadFromFile failed: %v", err)
	}

	services := bridge.parser.GetServices()
	if len(services) != 2 {
		t.Fatalf("expected 2 services, got %d", len(services))
	}
}

func TestListMethods(t *testing.T) {
	// Create a temporary proto file
	tmpDir := t.TempDir()
	protoContent := `
syntax = "proto3";
package test;

message Req {}
message Resp {}

service TestService {
    rpc UnaryMethod (Req) returns (Resp);
    rpc ServerStream (Req) returns (stream Resp);
}
`
	if err := os.WriteFile(filepath.Join(tmpDir, "test.proto"), []byte(protoContent), 0644); err != nil {
		t.Fatalf("failed to write proto file: %v", err)
	}

	// Change to temp directory
	chdirForTest(t, tmpDir)

	bridge := &Bridge{parser: proto.NewParser()}
	if err := bridge.parser.LoadFromFile("test.proto"); err != nil {
		t.Fatalf("LoadFromFile failed: %v", err)
	}

	methods, err := bridge.parser.GetMethods("test.TestService")
	if err != nil {
		t.Fatalf("GetMethods failed: %v", err)
	}

	if len(methods) != 2 {
		t.Fatalf("expected 2 methods, got %d", len(methods))
	}

	// Verify method types
	methodTypes := make(map[string]string)
	for _, m := range methods {
		methodTypes[m.Name] = m.Type
	}

	if methodTypes["UnaryMethod"] != "unary" {
		t.Errorf("expected UnaryMethod to be unary, got %s", methodTypes["UnaryMethod"])
	}
	if methodTypes["ServerStream"] != "server_stream" {
		t.Errorf("expected ServerStream to be server_stream, got %s", methodTypes["ServerStream"])
	}
}

func TestEncodeDecode(t *testing.T) {
	// Create a temporary proto file
	tmpDir := t.TempDir()
	protoContent := `
syntax = "proto3";
package test;

message TestRequest {
    string name = 1;
    int32 id = 2;
}

message TestResponse {
    string message = 1;
    bool success = 2;
}

service TestService {
    rpc TestMethod (TestRequest) returns (TestResponse);
}
`
	if err := os.WriteFile(filepath.Join(tmpDir, "test.proto"), []byte(protoContent), 0644); err != nil {
		t.Fatalf("failed to write proto file: %v", err)
	}

	// Change to temp directory
	chdirForTest(t, tmpDir)

	bridge := &Bridge{parser: proto.NewParser()}
	if err := bridge.parser.LoadFromFile("test.proto"); err != nil {
		t.Fatalf("LoadFromFile failed: %v", err)
	}

	// Test getting input/output types
	inputDesc, err := bridge.parser.GetInputType("test.TestService", "TestMethod")
	if err != nil {
		t.Fatalf("GetInputType failed: %v", err)
	}

	outputDesc, err := bridge.parser.GetOutputType("test.TestService", "TestMethod")
	if err != nil {
		t.Fatalf("GetOutputType failed: %v", err)
	}

	if inputDesc == nil {
		t.Fatal("inputDesc is nil")
	}
	if outputDesc == nil {
		t.Fatal("outputDesc is nil")
	}

	// Verify type names
	if inputDesc.GetFullyQualifiedName() != "test.TestRequest" {
		t.Errorf("expected input type 'test.TestRequest', got '%s'", inputDesc.GetFullyQualifiedName())
	}
	if outputDesc.GetFullyQualifiedName() != "test.TestResponse" {
		t.Errorf("expected output type 'test.TestResponse', got '%s'", outputDesc.GetFullyQualifiedName())
	}
}

func TestErrorHandling(t *testing.T) {
	// Test last_error with no error
	err := getLastError()
	if err != "" {
		t.Errorf("expected no error, got: %s", err)
	}

	// Test setting and getting error
	testErr := "test error message"
	setLastError(nil)
	setLastError(nil)
	setLastError(nil)
	lastError = testErr

	err = getLastError()
	if err != testErr {
		t.Errorf("expected '%s', got: %s", testErr, err)
	}
}

func TestInvalidProtoFile(t *testing.T) {
	bridge := &Bridge{parser: proto.NewParser()}

	// Try to load non-existent file
	err := bridge.parser.LoadFromFile("/nonexistent/file.proto")
	if err == nil {
		t.Error("expected failure for non-existent file")
	}
}

func TestInvalidMethodName(t *testing.T) {
	// Create a temporary proto file
	tmpDir := t.TempDir()
	protoContent := `
syntax = "proto3";
package test;

message Req {}
message Resp {}

service TestService {
    rpc TestMethod (Req) returns (Resp);
}
`
	if err := os.WriteFile(filepath.Join(tmpDir, "test.proto"), []byte(protoContent), 0644); err != nil {
		t.Fatalf("failed to write proto file: %v", err)
	}

	// Change to temp directory
	chdirForTest(t, tmpDir)

	bridge := &Bridge{parser: proto.NewParser()}
	if err := bridge.parser.LoadFromFile("test.proto"); err != nil {
		t.Fatalf("LoadFromFile failed: %v", err)
	}

	// Test invalid method name format
	_, err := bridge.parser.GetMethod("InvalidMethodName", "test")
	if err == nil {
		t.Error("expected error for invalid method name")
	}
}

func TestThreadSafety(t *testing.T) {
	// Create multiple bridges and verify store operations
	const numBridges = 10
	handles := make([]uintptr, numBridges)

	for i := 0; i < numBridges; i++ {
		bridge := &Bridge{parser: proto.NewParser()}
		handles[i] = store.add(bridge)
	}

	// Verify all handles exist
	for _, h := range handles {
		_, ok := store.get(h)
		if !ok {
			t.Errorf("handle %d should exist", h)
		}
	}

	// Free all bridges
	for _, h := range handles {
		store.remove(h)
	}

	// Verify all handles are freed
	for _, h := range handles {
		_, ok := store.get(h)
		if ok {
			t.Errorf("handle %d should have been freed", h)
		}
	}
}

// Test JSON marshaling functions
func TestJSONMarshaling(t *testing.T) {
	// Test ServiceListResponse
	response := ServiceListResponse{
		Services: []proto.ServiceInfo{
			{Name: "Service1", FullName: "test.Service1"},
			{Name: "Service2", FullName: "test.Service2"},
		},
	}
	data := toJSONString(response)
	if data == "" {
		t.Error("toJSONString returned empty string")
	}

	// Verify it's valid JSON
	var parsed ServiceListResponse
	if err := json.Unmarshal([]byte(data), &parsed); err != nil {
		t.Errorf("toJSONString produced invalid JSON: %v", err)
	}
	if len(parsed.Services) != 2 {
		t.Errorf("expected 2 services, got %d", len(parsed.Services))
	}

	// Test MethodListResponse
	methodResponse := MethodListResponse{
		Methods: []proto.MethodInfo{
			{Name: "Method1", Type: "unary"},
			{Name: "Method2", Type: "server_stream"},
		},
	}
	methodData := toJSONString(methodResponse)
	if methodData == "" {
		t.Error("toJSONString returned empty string for methods")
	}
}

// Test TLSConfig parsing
func TestTLSConfigParsing(t *testing.T) {
	jsonData := `{"insecure": true, "cert_path": "/path/to/cert", "key_path": "/path/to/key"}`
	var config TLSConfig
	if err := json.Unmarshal([]byte(jsonData), &config); err != nil {
		t.Fatalf("failed to parse TLS config: %v", err)
	}

	if !config.Insecure {
		t.Error("expected Insecure to be true")
	}
	if config.CertPath != "/path/to/cert" {
		t.Errorf("expected CertPath '/path/to/cert', got '%s'", config.CertPath)
	}
	if config.KeyPath != "/path/to/key" {
		t.Errorf("expected KeyPath '/path/to/key', got '%s'", config.KeyPath)
	}
}

func TestParseMethodNameSupportsMultipleFormats(t *testing.T) {
	testCases := []struct {
		name           string
		input          string
		expectedSvc    string
		expectedMethod string
	}{
		{
			name:           "service slash method",
			input:          "demo.public.PublicService/GetUserAgreement",
			expectedSvc:    "demo.public.PublicService",
			expectedMethod: "GetUserAgreement",
		},
		{
			name:           "leading slash",
			input:          "/demo.public.PublicService/GetUserAgreement",
			expectedSvc:    "demo.public.PublicService",
			expectedMethod: "GetUserAgreement",
		},
		{
			name:           "dot notation",
			input:          "demo.public.PublicService.GetUserAgreement",
			expectedSvc:    "demo.public.PublicService",
			expectedMethod: "GetUserAgreement",
		},
	}

	for _, testCase := range testCases {
		t.Run(testCase.name, func(t *testing.T) {
			serviceName, methodName, err := parseMethodName(testCase.input)
			if err != nil {
				t.Fatalf("parseMethodName failed: %v", err)
			}

			if serviceName != testCase.expectedSvc {
				t.Fatalf("service mismatch: got=%s want=%s", serviceName, testCase.expectedSvc)
			}
			if methodName != testCase.expectedMethod {
				t.Fatalf("method mismatch: got=%s want=%s", methodName, testCase.expectedMethod)
			}
		})
	}
}

func TestParseMethodNameRejectsInvalidFormat(t *testing.T) {
	_, _, err := parseMethodName("InvalidMethodName")
	if err == nil {
		t.Fatal("expected parseMethodName to reject invalid format")
	}
}

// TestNewCBufferFromBytesSupportsEmptyPayload 验证空消息不会触发越界崩溃。
func TestNewCBufferFromBytesSupportsEmptyPayload(t *testing.T) {
	buf, err := newCBufferFromBytes([]byte{})
	if err != nil {
		t.Fatalf("newCBufferFromBytes failed: %v", err)
	}
	if buf == nil {
		t.Fatal("expected non-nil buffer")
	}
	if buf.data == nil {
		t.Fatal("expected non-nil buffer.data for empty payload")
	}
	if got := int(buf.len); got != 0 {
		t.Fatalf("expected len=0 for empty payload, got %d", got)
	}

	free_buffer(unsafe.Pointer(buf))
}

// TestNewCBufferFromBytesCopiesPayload 验证非空消息的字节内容会被完整复制。
func TestNewCBufferFromBytesCopiesPayload(t *testing.T) {
	expected := []byte{0x01, 0x02, 0x7f, 0xff}
	buf, err := newCBufferFromBytes(expected)
	if err != nil {
		t.Fatalf("newCBufferFromBytes failed: %v", err)
	}
	if buf == nil || buf.data == nil {
		t.Fatal("expected non-nil C buffer and data")
	}
	if got := int(buf.len); got != len(expected) {
		t.Fatalf("expected len=%d, got %d", len(expected), got)
	}

	actual := unsafe.Slice((*byte)(unsafe.Pointer(buf.data)), len(expected))
	for index, value := range expected {
		if actual[index] != value {
			t.Fatalf("payload mismatch at index %d: got=%d want=%d", index, actual[index], value)
		}
	}

	free_buffer(unsafe.Pointer(buf))
}
