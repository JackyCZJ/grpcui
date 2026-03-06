package proto

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"

	"github.com/jhump/protoreflect/desc"            //nolint:staticcheck // 当前动态描述符解析仍依赖 jhump 生态。
	"github.com/jhump/protoreflect/desc/protoparse" //nolint:staticcheck // 当前 proto 文件解析流程依赖该实现。
	"github.com/jhump/protoreflect/grpcreflect"
	"google.golang.org/grpc"
	"google.golang.org/grpc/reflection/grpc_reflection_v1alpha"
	"google.golang.org/protobuf/encoding/protojson"
	"google.golang.org/protobuf/proto"
	"google.golang.org/protobuf/types/descriptorpb"
	"google.golang.org/protobuf/types/dynamicpb"
)

// Parser handles proto file parsing and reflection
type Parser struct {
	fileDescs map[string]*desc.FileDescriptor
	msgTypes  map[string]*desc.MessageDescriptor
	services  map[string]*desc.ServiceDescriptor
	methods   map[string]*desc.MethodDescriptor
	source    string // "file" or "reflection"
}

// NewParser creates a new Parser instance
func NewParser() *Parser {
	return &Parser{
		fileDescs: make(map[string]*desc.FileDescriptor),
		msgTypes:  make(map[string]*desc.MessageDescriptor),
		services:  make(map[string]*desc.ServiceDescriptor),
		methods:   make(map[string]*desc.MethodDescriptor),
	}
}

// resetState 清空解析器内的描述符索引缓存。
//
// 设计目的：
// 1) 每次重新加载 proto（文件或 reflection）都应以“全量替换”语义执行；
// 2) 避免旧项目服务残留到新项目，导致 UI 出现“方法混合”；
// 3) 保持 `GetServices/GetMethod` 等查询只返回当前最近一次成功加载的数据集。
func (p *Parser) resetState() {
	p.fileDescs = make(map[string]*desc.FileDescriptor)
	p.msgTypes = make(map[string]*desc.MessageDescriptor)
	p.services = make(map[string]*desc.ServiceDescriptor)
	p.methods = make(map[string]*desc.MethodDescriptor)
	p.source = ""
}

// LoadFromFile 兼容旧调用方式：仅传 proto 文件路径时仍可正常解析。
func (p *Parser) LoadFromFile(paths ...string) error {
	return p.LoadFromFileWithImports(paths, nil)
}

// normalizeNonEmptyProtoPaths 会过滤空白路径并执行 filepath.Clean，
// 让后续 import 路径推导与解析目标选择基于统一、稳定的路径表示。
func normalizeNonEmptyProtoPaths(paths []string) []string {
	normalized := make([]string, 0, len(paths))
	for _, rawPath := range paths {
		trimmed := strings.TrimSpace(rawPath)
		if trimmed == "" {
			continue
		}
		normalized = append(normalized, filepath.Clean(trimmed))
	}
	return normalized
}

// addImportPathIfPresent 在路径非空且不是当前目录占位时，
// 将其加入 importPathSet，避免重复与无效路径污染解析配置。
func addImportPathIfPresent(importPath string, importPathSet map[string]struct{}) {
	trimmed := strings.TrimSpace(importPath)
	if trimmed == "" || trimmed == "." {
		return
	}

	cleaned := filepath.Clean(trimmed)
	if cleaned == "." {
		return
	}

	importPathSet[cleaned] = struct{}{}
}

// relativePathWithinBase 尝试把 absolutePath 转成相对 baseDir 的路径。
//
// 返回 true 表示文件确实位于 baseDir 内部（不含 `..` 逃逸），
// 调用方可安全地把结果作为 proto 逻辑文件名使用。
func relativePathWithinBase(absolutePath string, baseDir string) (string, bool) {
	relative, err := filepath.Rel(baseDir, absolutePath)
	if err != nil || relative == "" || relative == "." {
		return "", false
	}

	if relative == ".." || strings.HasPrefix(relative, ".."+string(filepath.Separator)) {
		return "", false
	}

	return relative, true
}

// chooseParseTargetForAbsolutePath 为绝对 proto 路径选择合适的逻辑文件名。
//
// 选择策略：
// 1. 只考虑“文件位于 import 根目录内”的候选；
// 2. 若存在带目录层级的候选（包含分隔符），优先选它们，避免退化成仅文件名；
// 3. 在候选中按相对路径长度最短优先，兼顾 import 命中概率与 source_path 可读性。
//
// 返回值：
// - parseTarget: 供 protoparse.ParseFiles 使用的逻辑路径（统一 `/` 分隔）
// - matched: 是否命中了可用 import 根目录
func chooseParseTargetForAbsolutePath(absolutePath string, importPaths []string) (parseTarget string, matched bool) {
	type candidate struct {
		relative string
		score    int
	}

	candidates := make([]candidate, 0, len(importPaths))
	for _, importPath := range importPaths {
		relative, ok := relativePathWithinBase(absolutePath, importPath)
		if !ok {
			continue
		}

		score := len(relative)
		if strings.Contains(relative, string(filepath.Separator)) {
			score -= 10_000
		}

		candidates = append(candidates, candidate{relative: relative, score: score})
	}

	if len(candidates) == 0 {
		return "", false
	}

	sort.Slice(candidates, func(left, right int) bool {
		if candidates[left].score != candidates[right].score {
			return candidates[left].score < candidates[right].score
		}
		return candidates[left].relative < candidates[right].relative
	})

	return filepath.ToSlash(candidates[0].relative), true
}

// LoadFromFileWithImports 从 proto 文件及额外 import 根目录加载描述信息。
//
// 该方法用于“导入目录”场景：前端传相对 proto 路径，后端把目录根作为 import path，
// 这样多级目录中的 `import "a/b.proto"` 才能稳定解析，同时服务可保留相对 source_path。
func (p *Parser) LoadFromFileWithImports(paths []string, extraImportPaths []string) error {
	normalizedProtoPaths := normalizeNonEmptyProtoPaths(paths)
	if len(normalizedProtoPaths) == 0 {
		return fmt.Errorf("no proto files provided")
	}

	parser := protoparse.Parser{
		ImportPaths:           []string{"."},
		IncludeSourceCodeInfo: true,
	}

	importPathSet := make(map[string]struct{})
	for _, importPath := range extraImportPaths {
		addImportPathIfPresent(importPath, importPathSet)
	}

	for _, path := range normalizedProtoPaths {
		if filepath.IsAbs(path) {
			continue
		}

		dir := filepath.Dir(path)
		if dir != "" && dir != "." {
			addImportPathIfPresent(dir, importPathSet)
		}
	}

	resolvedImportPaths := make([]string, 0, len(importPathSet))
	for dir := range importPathSet {
		resolvedImportPaths = append(resolvedImportPaths, dir)
	}
	sort.Strings(resolvedImportPaths)

	parseTargets := make([]string, 0, len(normalizedProtoPaths))
	for _, path := range normalizedProtoPaths {
		if !filepath.IsAbs(path) {
			parseTargets = append(parseTargets, filepath.ToSlash(path))
			continue
		}

		if parseTarget, matched := chooseParseTargetForAbsolutePath(path, resolvedImportPaths); matched {
			parseTargets = append(parseTargets, parseTarget)
			continue
		}

		// 若外部未提供有效 import 根目录，回退到“文件所在目录 + 文件名”策略，
		// 保证绝对路径至少可被加载（适用于无额外 import 的单文件 proto）。
		fallbackDir := filepath.Dir(path)
		addImportPathIfPresent(fallbackDir, importPathSet)
		parseTargets = append(parseTargets, filepath.Base(path))
	}

	if len(importPathSet) != len(resolvedImportPaths) {
		resolvedImportPaths = resolvedImportPaths[:0]
		for dir := range importPathSet {
			resolvedImportPaths = append(resolvedImportPaths, dir)
		}
		sort.Strings(resolvedImportPaths)
	}

	parser.ImportPaths = append(parser.ImportPaths, resolvedImportPaths...)

	fileDescs, err := parser.ParseFiles(parseTargets...)
	if err != nil {
		return fmt.Errorf("failed to parse proto files: %w", err)
	}

	// 每次加载文件都先清空旧缓存，确保服务集合不会跨项目叠加。
	p.resetState()
	for _, fd := range fileDescs {
		p.fileDescs[fd.GetName()] = fd
		p.indexFile(fd)
	}

	p.source = "file"
	return nil
}

// LoadFromReflection loads proto definitions from gRPC server reflection
func (p *Parser) LoadFromReflection(conn *grpc.ClientConn) error {
	//nolint:staticcheck // 需要兼容目标服务常见的 v1alpha 反射接口。
	client := grpcreflect.NewClient(context.Background(), grpc_reflection_v1alpha.NewServerReflectionClient(conn))
	defer client.Reset()

	// Get list of services
	services, err := client.ListServices()
	if err != nil {
		return fmt.Errorf("failed to list services: %w", err)
	}

	// reflection 加载同样采用“全量替换”语义，避免旧 descriptor 与新结果混合。
	p.resetState()
	for _, svcName := range services {
		// Skip reflection service itself
		if svcName == "grpc.reflection.v1alpha.ServerReflection" {
			continue
		}

		svcDesc, err := client.ResolveService(svcName)
		if err != nil {
			continue // Skip services we can't resolve
		}

		p.services[svcName] = svcDesc
		p.indexService(svcDesc)

		// Index file descriptor for this service
		fileDesc := svcDesc.GetFile()
		if fileDesc != nil {
			p.fileDescs[fileDesc.GetName()] = fileDesc
			p.indexFile(fileDesc)
		}
	}

	p.source = "reflection"
	return nil
}

// indexFile indexes all types from a file descriptor
func (p *Parser) indexFile(fd *desc.FileDescriptor) {
	// Index messages
	for _, msg := range fd.GetMessageTypes() {
		p.indexMessage(msg, fd.GetPackage())
	}

	// Index services
	for _, svc := range fd.GetServices() {
		p.services[svc.GetFullyQualifiedName()] = svc
		p.indexService(svc)
	}
}

// indexMessage recursively indexes a message and its nested types
func (p *Parser) indexMessage(msg *desc.MessageDescriptor, pkg string) {
	fullName := msg.GetFullyQualifiedName()
	p.msgTypes[fullName] = msg

	// Also index with leading dot for compatibility
	p.msgTypes["."+fullName] = msg

	// Index nested messages
	for _, nested := range msg.GetNestedMessageTypes() {
		p.indexMessage(nested, pkg)
	}
}

// indexService indexes all methods from a service
func (p *Parser) indexService(svc *desc.ServiceDescriptor) {
	for _, method := range svc.GetMethods() {
		fullMethod := fmt.Sprintf("/%s/%s", svc.GetFullyQualifiedName(), method.GetName())
		p.methods[fullMethod] = method
		p.methods[method.GetFullyQualifiedName()] = method
	}
}

// GetServices returns all available services
func (p *Parser) GetServices() []ServiceInfo {
	var services []ServiceInfo
	for _, svc := range p.services {
		sourcePath := ""
		if fileDesc := svc.GetFile(); fileDesc != nil {
			sourcePath = filepath.ToSlash(fileDesc.GetName())
		}

		services = append(services, ServiceInfo{
			Name:       svc.GetName(),
			FullName:   svc.GetFullyQualifiedName(),
			SourcePath: sourcePath,
			Methods:    p.getMethodInfos(svc),
		})
	}
	return services
}

// GetService returns a specific service by name
func (p *Parser) GetService(name string) (*desc.ServiceDescriptor, error) {
	svc, ok := p.services[name]
	if !ok {
		// Try with leading dot
		svc, ok = p.services["."+name]
	}
	if !ok {
		return nil, fmt.Errorf("service not found: %s", name)
	}
	return svc, nil
}

// GetMethods returns all methods for a service
func (p *Parser) GetMethods(serviceName string) ([]MethodInfo, error) {
	svc, err := p.GetService(serviceName)
	if err != nil {
		return nil, err
	}
	return p.getMethodInfos(svc), nil
}

func (p *Parser) getMethodInfos(svc *desc.ServiceDescriptor) []MethodInfo {
	var methods []MethodInfo
	for _, method := range svc.GetMethods() {
		methodType := "unary"
		switch {
		case method.IsClientStreaming() && method.IsServerStreaming():
			methodType = "bidi_stream"
		case method.IsClientStreaming():
			methodType = "client_stream"
		case method.IsServerStreaming():
			methodType = "server_stream"
		}
		methods = append(methods, MethodInfo{
			Name:            method.GetName(),
			FullName:        method.GetFullyQualifiedName(),
			InputType:       method.GetInputType().GetFullyQualifiedName(),
			OutputType:      method.GetOutputType().GetFullyQualifiedName(),
			Type:            methodType,
			ClientStreaming: method.IsClientStreaming(),
			ServerStreaming: method.IsServerStreaming(),
		})
	}
	return methods
}

// buildServiceNameCandidates 将调用方传入的 service 名标准化为多种候选形式，
// 兼容“全限定名/短服务名/带前缀斜杠”的混用场景。
func buildServiceNameCandidates(serviceName string) []string {
	normalized := strings.TrimSpace(serviceName)
	normalized = strings.TrimPrefix(normalized, "/")
	normalized = strings.TrimPrefix(normalized, ".")
	if normalized == "" {
		return nil
	}

	candidates := []string{normalized}
	if dot := strings.LastIndex(normalized, "."); dot >= 0 && dot+1 < len(normalized) {
		shortName := normalized[dot+1:]
		if shortName != "" && shortName != normalized {
			candidates = append(candidates, shortName)
		}
	}

	return candidates
}

// serviceNameMatches 用于判断服务名候选是否与当前 descriptor 匹配。
// 规则支持：全名精确匹配、短名匹配、以及“全名后缀匹配短名”。
func serviceNameMatches(serviceFullName, serviceShortName string, candidates []string) bool {
	for _, candidate := range candidates {
		if candidate == serviceFullName || candidate == serviceShortName {
			return true
		}

		if strings.HasSuffix(serviceFullName, "."+candidate) {
			return true
		}
	}

	return false
}

// GetMethod returns a specific method descriptor
func (p *Parser) GetMethod(serviceName, methodName string) (*desc.MethodDescriptor, error) {
	methodName = strings.TrimSpace(methodName)
	serviceCandidates := buildServiceNameCandidates(serviceName)
	if len(serviceCandidates) == 0 || methodName == "" {
		return nil, fmt.Errorf("method not found: /%s/%s", strings.TrimSpace(serviceName), methodName)
	}

	// 优先走精确 key 命中，保持已有性能与行为。
	for _, candidate := range serviceCandidates {
		fullMethod := fmt.Sprintf("/%s/%s", candidate, methodName)
		if method, ok := p.methods[fullMethod]; ok {
			return method, nil
		}
	}

	// 回退到 descriptor 级匹配，兼容服务名含/不含 package 的混用场景。
	var matched *desc.MethodDescriptor
	seen := make(map[string]struct{})
	for _, method := range p.methods {
		methodFullName := method.GetFullyQualifiedName()
		if _, exists := seen[methodFullName]; exists {
			continue
		}
		seen[methodFullName] = struct{}{}

		if method.GetName() != methodName {
			continue
		}

		serviceDesc := method.GetService()
		if serviceDesc == nil {
			continue
		}

		if !serviceNameMatches(
			serviceDesc.GetFullyQualifiedName(),
			serviceDesc.GetName(),
			serviceCandidates,
		) {
			continue
		}

		if matched != nil && matched.GetFullyQualifiedName() != methodFullName {
			return nil, fmt.Errorf("ambiguous method: service=%s method=%s", serviceName, methodName)
		}
		matched = method
	}

	if matched != nil {
		return matched, nil
	}

	return nil, fmt.Errorf("method not found: /%s/%s", strings.TrimPrefix(strings.TrimSpace(serviceName), "/"), methodName)
}

// CreateMessage creates a new dynamic message of the given type
func (p *Parser) CreateMessage(typeName string) (*dynamicpb.Message, error) {
	// Normalize type name
	typeName = strings.TrimPrefix(typeName, ".")

	desc, ok := p.msgTypes[typeName]
	if !ok {
		// Try with leading dot
		desc, ok = p.msgTypes["."+typeName]
	}
	if !ok {
		return nil, fmt.Errorf("message type not found: %s", typeName)
	}

	return dynamicpb.NewMessage(desc.UnwrapMessage()), nil
}

// CreateMessageFromDesc creates a message from a descriptor
func CreateMessageFromDesc(desc *desc.MessageDescriptor) *dynamicpb.Message {
	return dynamicpb.NewMessage(desc.UnwrapMessage())
}

// MessageToJSON converts a dynamic message to JSON
func MessageToJSON(msg *dynamicpb.Message) ([]byte, error) {
	return protojson.Marshal(msg)
}

// JSONToMessage parses JSON into a dynamic message
func (p *Parser) JSONToMessage(jsonData []byte, typeName string) (*dynamicpb.Message, error) {
	msg, err := p.CreateMessage(typeName)
	if err != nil {
		return nil, err
	}

	if err := protojson.Unmarshal(jsonData, msg); err != nil {
		return nil, fmt.Errorf("failed to unmarshal JSON: %w", err)
	}

	return msg, nil
}

// JSONToMessageWithDesc parses JSON into a message using a descriptor
func JSONToMessageWithDesc(jsonData []byte, desc *desc.MessageDescriptor) (*dynamicpb.Message, error) {
	msg := CreateMessageFromDesc(desc)
	if err := protojson.Unmarshal(jsonData, msg); err != nil {
		return nil, fmt.Errorf("failed to unmarshal JSON: %w", err)
	}
	return msg, nil
}

// GetMessageDescriptor returns the descriptor for a message type
func (p *Parser) GetMessageDescriptor(typeName string) (*desc.MessageDescriptor, error) {
	typeName = strings.TrimPrefix(typeName, ".")
	desc, ok := p.msgTypes[typeName]
	if !ok {
		return nil, fmt.Errorf("message type not found: %s", typeName)
	}
	return desc, nil
}

// GetInputType returns the input message descriptor for a method
func (p *Parser) GetInputType(serviceName, methodName string) (*desc.MessageDescriptor, error) {
	method, err := p.GetMethod(serviceName, methodName)
	if err != nil {
		return nil, err
	}
	return method.GetInputType(), nil
}

// GetOutputType returns the output message descriptor for a method
func (p *Parser) GetOutputType(serviceName, methodName string) (*desc.MessageDescriptor, error) {
	method, err := p.GetMethod(serviceName, methodName)
	if err != nil {
		return nil, err
	}
	return method.GetOutputType(), nil
}

// IsStreamingMethod checks if a method is streaming
func (p *Parser) IsStreamingMethod(serviceName, methodName string) (clientStreaming, serverStreaming bool, err error) {
	method, err := p.GetMethod(serviceName, methodName)
	if err != nil {
		return false, false, err
	}
	return method.IsClientStreaming(), method.IsServerStreaming(), nil
}

// Source returns the source of proto definitions ("file" or "reflection")
func (p *Parser) Source() string {
	return p.source
}

// ServiceInfo holds information about a service
type ServiceInfo struct {
	Name       string       `json:"name"`
	FullName   string       `json:"full_name"`
	SourcePath string       `json:"source_path,omitempty"`
	Methods    []MethodInfo `json:"methods"`
}

// MethodInfo holds information about a method
type MethodInfo struct {
	Name            string `json:"name"`
	FullName        string `json:"full_name"`
	InputType       string `json:"input_type"`
	OutputType      string `json:"output_type"`
	Type            string `json:"type"`
	ClientStreaming bool   `json:"client_streaming"`
	ServerStreaming bool   `json:"server_streaming"`
}

// MessageSchema 描述一个 protobuf 消息结构，供前端字段化请求体渲染使用。
//
// 前端会据此生成“按字段编辑”的请求体面板，避免用户手写整段 JSON。
type MessageSchema struct {
	TypeName string             `json:"type_name"`
	Fields   []MessageFieldInfo `json:"fields"`
}

// MessageFieldInfo 描述消息字段的可视化编辑信息。
//
// 字段说明：
// - kind: scalar/enum/message/map
// - type: 标量类型名或完整类型名（如 `pkg.Message`）
// - repeated/map: 容器字段属性
// - fields/enum_values: 复合类型的补充信息
type MessageFieldInfo struct {
	Name               string             `json:"name"`
	JSONName           string             `json:"json_name"`
	Kind               string             `json:"kind"`
	Type               string             `json:"type"`
	Repeated           bool               `json:"repeated"`
	Required           bool               `json:"required"`
	Optional           bool               `json:"optional"`
	OneOf              string             `json:"one_of,omitempty"`
	EnumValues         []string           `json:"enum_values,omitempty"`
	Fields             []MessageFieldInfo `json:"fields,omitempty"`
	Map                bool               `json:"map,omitempty"`
	MapKeyType         string             `json:"map_key_type,omitempty"`
	MapValueKind       string             `json:"map_value_kind,omitempty"`
	MapValueType       string             `json:"map_value_type,omitempty"`
	MapValueEnumValues []string           `json:"map_value_enum_values,omitempty"`
	MapValueFields     []MessageFieldInfo `json:"map_value_fields,omitempty"`
}

const maxSchemaDepth = 8

// protobufScalarTypeName 将 protobuf 字段类型枚举转换为前端可读的短类型名。
func protobufScalarTypeName(fieldType descriptorpb.FieldDescriptorProto_Type) string {
	return strings.ToLower(strings.TrimPrefix(fieldType.String(), "TYPE_"))
}

// enumValueNames 提取枚举值名称列表，供前端渲染下拉选项。
func enumValueNames(enumDesc *desc.EnumDescriptor) []string {
	if enumDesc == nil {
		return nil
	}

	values := enumDesc.GetValues()
	result := make([]string, 0, len(values))
	for _, value := range values {
		result = append(result, value.GetName())
	}
	return result
}

// buildMessageSchemaFields 递归构建消息字段结构。
//
// 为防止自引用消息导致无限递归，使用 visiting 记录当前递归路径；
// 同时通过 depth 上限兜底，保证复杂 proto 项目下也能稳定返回。
func buildMessageSchemaFields(
	msg *desc.MessageDescriptor,
	visiting map[string]struct{},
	depth int,
) []MessageFieldInfo {
	if msg == nil || depth > maxSchemaDepth {
		return nil
	}

	msgName := msg.GetFullyQualifiedName()
	if _, exists := visiting[msgName]; exists {
		return nil
	}
	visiting[msgName] = struct{}{}
	defer delete(visiting, msgName)

	fields := msg.GetFields()
	result := make([]MessageFieldInfo, 0, len(fields))
	for _, field := range fields {
		result = append(result, buildFieldSchema(field, visiting, depth+1))
	}

	return result
}

// buildFieldSchema 将单个 protobuf 字段转换为前端可消费的字段描述。
func buildFieldSchema(
	field *desc.FieldDescriptor,
	visiting map[string]struct{},
	depth int,
) MessageFieldInfo {
	info := MessageFieldInfo{
		Name:     field.GetName(),
		JSONName: field.GetJSONName(),
		Repeated: field.IsRepeated() && !field.IsMap(),
		Required: field.IsRequired(),
		Optional: field.IsProto3Optional(),
	}

	if oneOf := field.GetOneOf(); oneOf != nil {
		info.OneOf = oneOf.GetName()
	}

	if field.IsMap() {
		info.Kind = "map"
		info.Type = "map"
		info.Map = true

		if keyField := field.GetMapKeyType(); keyField != nil {
			info.MapKeyType = protobufScalarTypeName(keyField.GetType())
		}

		if valueField := field.GetMapValueType(); valueField != nil {
			valueType := valueField.GetType()
			switch valueType {
			case descriptorpb.FieldDescriptorProto_TYPE_MESSAGE:
				info.MapValueKind = "message"
				if msgType := valueField.GetMessageType(); msgType != nil {
					info.MapValueType = msgType.GetFullyQualifiedName()
					info.MapValueFields = buildMessageSchemaFields(msgType, visiting, depth+1)
				}
			case descriptorpb.FieldDescriptorProto_TYPE_ENUM:
				info.MapValueKind = "enum"
				if enumType := valueField.GetEnumType(); enumType != nil {
					info.MapValueType = enumType.GetFullyQualifiedName()
					info.MapValueEnumValues = enumValueNames(enumType)
				}
			default:
				info.MapValueKind = "scalar"
				info.MapValueType = protobufScalarTypeName(valueType)
			}
		}

		return info
	}

	fieldType := field.GetType()
	switch fieldType {
	case descriptorpb.FieldDescriptorProto_TYPE_MESSAGE:
		info.Kind = "message"
		if msgType := field.GetMessageType(); msgType != nil {
			info.Type = msgType.GetFullyQualifiedName()
			info.Fields = buildMessageSchemaFields(msgType, visiting, depth+1)
		}
	case descriptorpb.FieldDescriptorProto_TYPE_ENUM:
		info.Kind = "enum"
		if enumType := field.GetEnumType(); enumType != nil {
			info.Type = enumType.GetFullyQualifiedName()
			info.EnumValues = enumValueNames(enumType)
		}
	default:
		info.Kind = "scalar"
		info.Type = protobufScalarTypeName(fieldType)
	}

	return info
}

// GetMethodInputSchema 返回指定方法入参消息的字段结构描述。
//
// 该能力用于前端“字段化请求体编辑器”：
// 前端拿到结构后可渲染表单，再序列化为 JSON 发送给 encode_request。
func (p *Parser) GetMethodInputSchema(serviceName, methodName string) (*MessageSchema, error) {
	method, err := p.GetMethod(serviceName, methodName)
	if err != nil {
		return nil, err
	}

	inputType := method.GetInputType()
	if inputType == nil {
		return nil, fmt.Errorf("input type not found: %s/%s", serviceName, methodName)
	}

	fields := buildMessageSchemaFields(inputType, make(map[string]struct{}), 0)
	return &MessageSchema{
		TypeName: inputType.GetFullyQualifiedName(),
		Fields:   fields,
	}, nil
}

// ParseProtoFile is a convenience function to parse a single proto file
func ParseProtoFile(path string) (*Parser, error) {
	p := NewParser()
	if err := p.LoadFromFile(path); err != nil {
		return nil, err
	}
	return p, nil
}

// ParseFromReflection is a convenience function to parse from server reflection
func ParseFromReflection(conn *grpc.ClientConn) (*Parser, error) {
	p := NewParser()
	if err := p.LoadFromReflection(conn); err != nil {
		return nil, err
	}
	return p, nil
}

// LoadFileDescriptorSet loads proto definitions from a FileDescriptorSet file
func (p *Parser) LoadFileDescriptorSet(path string) error {
	data, err := os.ReadFile(path)
	if err != nil {
		return fmt.Errorf("failed to read file descriptor set: %w", err)
	}

	var fdSet descriptorpb.FileDescriptorSet
	if err := proto.Unmarshal(data, &fdSet); err != nil {
		return fmt.Errorf("failed to unmarshal file descriptor set: %w", err)
	}

	for _, fdProto := range fdSet.File {
		fd, err := desc.CreateFileDescriptor(fdProto)
		if err != nil {
			continue // Skip files we can't create
		}
		p.fileDescs[fd.GetName()] = fd
		p.indexFile(fd)
	}

	p.source = "file"
	return nil
}
