package proto

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// writeProtoFixture 写入测试所需的 proto 文件夹结构，
// 便于在单元测试里复现“相对路径 + import 根目录”解析流程。
func writeProtoFixture(t *testing.T, path string, content string) {
	t.Helper()

	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatalf("创建目录失败: %v", err)
	}

	if err := os.WriteFile(path, []byte(content), 0o644); err != nil {
		t.Fatalf("写入 proto 文件失败: %v", err)
	}
}

func TestLoadFromFileWithImportsResolvesRelativeImportAndTracksSourcePath(t *testing.T) {
	tmpDir := t.TempDir()

	writeProtoFixture(t, filepath.Join(tmpDir, "common", "types.proto"), `syntax = "proto3";
package demo.common;
message Empty {}
`)

	writeProtoFixture(t, filepath.Join(tmpDir, "user", "user.proto"), `syntax = "proto3";
package demo.user;
import "common/types.proto";
service UserService {
  rpc Ping(demo.common.Empty) returns (demo.common.Empty);
}
`)

	parser := NewParser()
	err := parser.LoadFromFileWithImports(
		[]string{"user/user.proto"},
		[]string{tmpDir},
	)
	if err != nil {
		t.Fatalf("加载 proto 目录失败: %v", err)
	}

	services := parser.GetServices()
	if len(services) != 1 {
		t.Fatalf("期望 1 个服务，实际 %d", len(services))
	}

	if services[0].FullName != "demo.user.UserService" {
		t.Fatalf("服务全名不匹配: %s", services[0].FullName)
	}

	if services[0].SourcePath != "user/user.proto" {
		t.Fatalf("source_path 不正确: %s", services[0].SourcePath)
	}
}

func TestLoadFromFileWithImportsRejectsEmptyPathList(t *testing.T) {
	parser := NewParser()
	err := parser.LoadFromFileWithImports(nil, nil)
	if err == nil {
		t.Fatalf("期望空路径输入返回错误")
	}

	if !strings.Contains(err.Error(), "no proto files") {
		t.Fatalf("错误信息不符合预期: %v", err)
	}
}

func TestLoadFromFileWithImportsSupportsRepositoryStyleImportPrefix(t *testing.T) {
	tmpDir := t.TempDir()
	protoRoot := filepath.Join(tmpDir, "common", "proto")

	writeProtoFixture(t, filepath.Join(protoRoot, "base.proto"), `syntax = "proto3";
package demo.base;
message Empty {}
`)

	writeProtoFixture(t, filepath.Join(protoRoot, "user", "service.proto"), `syntax = "proto3";
package demo.user;
import "common/proto/base.proto";
service UserService {
  rpc Ping(demo.base.Empty) returns (demo.base.Empty);
}
`)

	parser := NewParser()
	err := parser.LoadFromFileWithImports(
		[]string{"user/service.proto"},
		[]string{protoRoot, filepath.Dir(protoRoot), tmpDir},
	)
	if err != nil {
		t.Fatalf("加载带仓库前缀 import 的 proto 失败: %v", err)
	}

	services := parser.GetServices()
	if len(services) != 1 {
		t.Fatalf("期望 1 个服务，实际 %d", len(services))
	}

	if services[0].SourcePath != "user/service.proto" {
		t.Fatalf("source_path 不正确: %s", services[0].SourcePath)
	}
}

func TestLoadFromFileWithImportsSupportsAbsoluteProtoPath(t *testing.T) {
	tmpDir := t.TempDir()
	protoRoot := filepath.Join(tmpDir, "common", "proto")

	writeProtoFixture(t, filepath.Join(protoRoot, "types", "enums.proto"), `syntax = "proto3";
package demo.types;
enum Status {
  STATUS_UNSPECIFIED = 0;
}
message PingPayload {
  Status status = 1;
}
`)

	servicePath := filepath.Join(protoRoot, "user", "service.proto")
	writeProtoFixture(t, servicePath, `syntax = "proto3";
package demo.user;
import "types/enums.proto";
service UserService {
  rpc Ping(demo.types.PingPayload) returns (demo.types.PingPayload);
}
`)

	parser := NewParser()
	err := parser.LoadFromFileWithImports(
		[]string{servicePath},
		[]string{
			filepath.Join(protoRoot, "user"),
			protoRoot,
			filepath.Dir(protoRoot),
		},
	)
	if err != nil {
		t.Fatalf("加载绝对路径 proto 失败: %v", err)
	}

	services := parser.GetServices()
	if len(services) != 1 {
		t.Fatalf("期望 1 个服务，实际 %d", len(services))
	}

	if services[0].FullName != "demo.user.UserService" {
		t.Fatalf("服务全名不匹配: %s", services[0].FullName)
	}
}

func TestLoadFromFileWithImportsFallsBackToAbsoluteFileDirectory(t *testing.T) {
	tmpDir := t.TempDir()
	servicePath := filepath.Join(tmpDir, "single", "standalone.proto")

	writeProtoFixture(t, servicePath, `syntax = "proto3";
package demo.single;
import "google/protobuf/empty.proto";
service StandaloneService {
  rpc Ping(google.protobuf.Empty) returns (google.protobuf.Empty);
}
`)

	parser := NewParser()
	err := parser.LoadFromFileWithImports([]string{servicePath}, nil)
	if err != nil {
		t.Fatalf("绝对路径单文件应可回退加载: %v", err)
	}

	services := parser.GetServices()
	if len(services) != 1 {
		t.Fatalf("期望 1 个服务，实际 %d", len(services))
	}

	if services[0].FullName != "demo.single.StandaloneService" {
		t.Fatalf("服务全名不匹配: %s", services[0].FullName)
	}
}

func TestLoadFromFileWithImportsReplacesPreviousDescriptors(t *testing.T) {
	tmpDir := t.TempDir()

	writeProtoFixture(t, filepath.Join(tmpDir, "alpha", "alpha.proto"), `syntax = "proto3";
package demo.alpha;
message Empty {}
service AlphaService {
  rpc Ping(Empty) returns (Empty);
}
`)

	writeProtoFixture(t, filepath.Join(tmpDir, "beta", "beta.proto"), `syntax = "proto3";
package demo.beta;
message Empty {}
service BetaService {
  rpc Ping(Empty) returns (Empty);
}
`)

	parser := NewParser()
	if err := parser.LoadFromFileWithImports(
		[]string{"alpha/alpha.proto"},
		[]string{tmpDir},
	); err != nil {
		t.Fatalf("首次加载 proto 失败: %v", err)
	}

	firstServices := parser.GetServices()
	if len(firstServices) != 1 || firstServices[0].FullName != "demo.alpha.AlphaService" {
		t.Fatalf("首次加载服务异常: %+v", firstServices)
	}

	if err := parser.LoadFromFileWithImports(
		[]string{"beta/beta.proto"},
		[]string{tmpDir},
	); err != nil {
		t.Fatalf("二次加载 proto 失败: %v", err)
	}

	secondServices := parser.GetServices()
	if len(secondServices) != 1 {
		t.Fatalf("期望仅保留二次加载服务，实际 %d", len(secondServices))
	}

	if secondServices[0].FullName != "demo.beta.BetaService" {
		t.Fatalf("二次加载服务不正确: %+v", secondServices)
	}
}

func TestGetMethodInputSchemaReturnsNestedFieldDefinitions(t *testing.T) {
	tmpDir := t.TempDir()

	writeProtoFixture(t, filepath.Join(tmpDir, "user", "user.proto"), `syntax = "proto3";
package demo.user;

enum Status {
  STATUS_UNSPECIFIED = 0;
  STATUS_ACTIVE = 1;
}

message Profile {
  string email = 1;
}

message CreateUserRequest {
  string name = 1;
  repeated int32 roles = 2;
  Profile profile = 3;
  Status status = 4;
}

message CreateUserResponse {
  string id = 1;
}

service UserService {
  rpc CreateUser(CreateUserRequest) returns (CreateUserResponse);
}
`)

	parser := NewParser()
	err := parser.LoadFromFileWithImports(
		[]string{"user/user.proto"},
		[]string{tmpDir},
	)
	if err != nil {
		t.Fatalf("加载 proto 失败: %v", err)
	}

	schema, err := parser.GetMethodInputSchema("demo.user.UserService", "CreateUser")
	if err != nil {
		t.Fatalf("获取方法入参 schema 失败: %v", err)
	}

	if schema.TypeName != "demo.user.CreateUserRequest" {
		t.Fatalf("schema 类型名不匹配: %s", schema.TypeName)
	}

	if len(schema.Fields) != 4 {
		t.Fatalf("期望 4 个字段，实际 %d", len(schema.Fields))
	}

	var (
		nameField    *MessageFieldInfo
		rolesField   *MessageFieldInfo
		profileField *MessageFieldInfo
		statusField  *MessageFieldInfo
	)

	for index := range schema.Fields {
		field := &schema.Fields[index]
		switch field.Name {
		case "name":
			nameField = field
		case "roles":
			rolesField = field
		case "profile":
			profileField = field
		case "status":
			statusField = field
		}
	}

	if nameField == nil || nameField.Kind != "scalar" || nameField.Type != "string" {
		t.Fatalf("name 字段 schema 不正确: %+v", nameField)
	}

	if rolesField == nil || !rolesField.Repeated || rolesField.Type != "int32" {
		t.Fatalf("roles 字段 schema 不正确: %+v", rolesField)
	}

	if profileField == nil || profileField.Kind != "message" || len(profileField.Fields) != 1 {
		t.Fatalf("profile 字段 schema 不正确: %+v", profileField)
	}

	if statusField == nil || statusField.Kind != "enum" || len(statusField.EnumValues) != 2 {
		t.Fatalf("status 字段 schema 不正确: %+v", statusField)
	}
}

func TestGetMethodSupportsShortServiceNameFallback(t *testing.T) {
	tmpDir := t.TempDir()

	writeProtoFixture(t, filepath.Join(tmpDir, "public", "public.proto"), `syntax = "proto3";
package demo.public;

message GetUserAgreementRequest {}
message GetUserAgreementResponse {}

service PublicService {
  rpc GetUserAgreement(GetUserAgreementRequest) returns (GetUserAgreementResponse);
}
`)

	parser := NewParser()
	err := parser.LoadFromFileWithImports(
		[]string{"public/public.proto"},
		[]string{tmpDir},
	)
	if err != nil {
		t.Fatalf("加载 proto 失败: %v", err)
	}

	methodFromFullName, err := parser.GetMethod("demo.public.PublicService", "GetUserAgreement")
	if err != nil {
		t.Fatalf("使用全限定服务名获取方法失败: %v", err)
	}

	methodFromShortName, err := parser.GetMethod("PublicService", "GetUserAgreement")
	if err != nil {
		t.Fatalf("使用短服务名获取方法失败: %v", err)
	}

	if methodFromShortName.GetFullyQualifiedName() != methodFromFullName.GetFullyQualifiedName() {
		t.Fatalf(
			"短服务名回退匹配到错误方法: got=%s want=%s",
			methodFromShortName.GetFullyQualifiedName(),
			methodFromFullName.GetFullyQualifiedName(),
		)
	}
}
