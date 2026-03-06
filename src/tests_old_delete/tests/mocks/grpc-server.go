package mocks

import (
	"context"
	"fmt"
	"net"
	"sync"

	"google.golang.org/grpc"
	"google.golang.org/grpc/codes"
	"google.golang.org/grpc/status"
	pb "google.golang.org/protobuf/runtime/protoimpl"
	"google.golang.org/protobuf/types/known/emptypb"
)

// Mock gRPC server for testing

type GreeterServer struct {
	UnimplementedGreeterServer
}

type StreamingServer struct {
	UnimplementedStreamingServiceServer
}

// Greeter service
type UnimplementedGreeterServer struct{}

func (s *UnimplementedGreeterServer) SayHello(ctx context.Context, req *HelloRequest) (*HelloReply, error) {
	return &HelloReply{Message: fmt.Sprintf("Hello, %s!", req.Name)}, nil
}

// Streaming service
type UnimplementedStreamingServiceServer struct{}

func (s *UnimplementedStreamingServiceServer) ServerStream(req *StreamRequest, stream StreamingService_ServerStreamServer) error {
	count := req.Count
	if count == 0 {
		count = 3
	}
	for i := int32(0); i < count; i++ {
		if err := stream.Send(&StreamResponse{
			Data:     fmt.Sprintf("message %d", i),
			Sequence: i,
		}); err != nil {
			return err
		}
	}
	return nil
}

func (s *UnimplementedStreamingServiceServer) ClientStream(stream StreamingService_ClientStreamServer) error {
	var messageCount int32
	for {
		_, err := stream.Recv()
		if err != nil {
			break
		}
		messageCount++
	}
	return stream.SendAndClose(&StreamResponse{
		Data:     fmt.Sprintf("Received %d messages", messageCount),
		Sequence: messageCount,
	})
}

func (s *UnimplementedStreamingServiceServer) BidiStream(stream StreamingService_BidiStreamServer) error {
	var sequence int32
	for {
		req, err := stream.Recv()
		if err != nil {
			return nil
		}
		if err := stream.Send(&StreamResponse{
			Data:     fmt.Sprintf("Echo: %s", req.Data),
			Sequence: sequence,
		}); err != nil {
			return err
		}
		sequence++
	}
}

func (s *UnimplementedStreamingServiceServer) ErrorStream(req *StreamRequest, stream StreamingService_ErrorStreamServer) error {
	return status.Error(codes.Internal, "Test error")
}

// Proto message types
type HelloRequest struct {
	state         protoimpl.MessageState
	sizeCache     protoimpl.SizeCache
	unknownFields protoimpl.UnknownFields

	Name string `protobuf:"bytes,1,opt,name=name,proto3" json:"name,omitempty"`
}

func (x *HelloRequest) Reset() {
	*x = HelloRequest{}
}

func (x *HelloRequest) String() string {
	return fmt.Sprintf("HelloRequest{Name: %s}", x.Name)
}

func (*HelloRequest) ProtoMessage() {}

func (x *HelloRequest) GetName() string {
	if x != nil {
		return x.Name
	}
	return ""
}

type HelloReply struct {
	state         protoimpl.MessageState
	sizeCache     protoimpl.SizeCache
	unknownFields protoimpl.UnknownFields

	Message string `protobuf:"bytes,1,opt,name=message,proto3" json:"message,omitempty"`
}

func (x *HelloReply) Reset() {
	*x = HelloReply{}
}

func (x *HelloReply) String() string {
	return fmt.Sprintf("HelloReply{Message: %s}", x.Message)
}

func (*HelloReply) ProtoMessage() {}

func (x *HelloReply) GetMessage() string {
	if x != nil {
		return x.Message
	}
	return ""
}

type StreamRequest struct {
	state         protoimpl.MessageState
	sizeCache     protoimpl.SizeCache
	unknownFields protoimpl.UnknownFields

	Data  string `protobuf:"bytes,1,opt,name=data,proto3" json:"data,omitempty"`
	Count int32  `protobuf:"varint,2,opt,name=count,proto3" json:"count,omitempty"`
}

func (x *StreamRequest) Reset() {
	*x = StreamRequest{}
}

func (x *StreamRequest) String() string {
	return fmt.Sprintf("StreamRequest{Data: %s, Count: %d}", x.Data, x.Count)
}

func (*StreamRequest) ProtoMessage() {}

func (x *StreamRequest) GetData() string {
	if x != nil {
		return x.Data
	}
	return ""
}

func (x *StreamRequest) GetCount() int32 {
	if x != nil {
		return x.Count
	}
	return 0
}

type StreamResponse struct {
	state         protoimpl.MessageState
	sizeCache     protoimpl.SizeCache
	unknownFields protoimpl.UnknownFields

	Data     string `protobuf:"bytes,1,opt,name=data,proto3" json:"data,omitempty"`
	Sequence int32  `protobuf:"varint,2,opt,name=sequence,proto3" json:"sequence,omitempty"`
}

func (x *StreamResponse) Reset() {
	*x = StreamResponse{}
}

func (x *StreamResponse) String() string {
	return fmt.Sprintf("StreamResponse{Data: %s, Sequence: %d}", x.Data, x.Sequence)
}

func (*StreamResponse) ProtoMessage() {}

func (x *StreamResponse) GetData() string {
	if x != nil {
		return x.Data
	}
	return ""
}

func (x *StreamResponse) GetSequence() int32 {
	if x != nil {
		return x.Sequence
	}
	return 0
}

// gRPC service interfaces
type GreeterServer interface {
	SayHello(context.Context, *HelloRequest) (*HelloReply, error)
}

type StreamingServiceServer interface {
	ServerStream(*StreamRequest, StreamingService_ServerStreamServer) error
	ClientStream(StreamingService_ClientStreamServer) error
	BidiStream(StreamingService_BidiStreamServer) error
	ErrorStream(*StreamRequest, StreamingService_ErrorStreamServer) error
}

type StreamingService_ServerStreamServer interface {
	Send(*StreamResponse) error
	grpc.ServerStream
}

type StreamingService_ClientStreamServer interface {
	Recv() (*StreamRequest, error)
	grpc.ServerStream
	SendAndClose(*StreamResponse) error
}

type StreamingService_BidiStreamServer interface {
	Send(*StreamResponse) error
	Recv() (*StreamRequest, error)
	grpc.ServerStream
}

type StreamingService_ErrorStreamServer interface {
	Send(*StreamResponse) error
	grpc.ServerStream
}

// Server registration functions
func RegisterGreeterServer(s *grpc.Server, srv GreeterServer) {
	s.RegisterService(&Greeter_ServiceDesc, srv)
}

func RegisterStreamingServiceServer(s *grpc.Server, srv StreamingServiceServer) {
	s.RegisterService(&StreamingService_ServiceDesc, srv)
}

var Greeter_ServiceDesc = grpc.ServiceDesc{
	ServiceName: "test.Greeter",
	HandlerType: (*GreeterServer)(nil),
	Methods: []grpc.MethodDesc{
		{
			MethodName: "SayHello",
			Handler:    _Greeter_SayHello_Handler,
		},
	},
	Streams:  []grpc.StreamDesc{},
	Metadata: "test.proto",
}

func _Greeter_SayHello_Handler(srv interface{}, ctx context.Context, dec func(interface{}) error, interceptor grpc.UnaryServerInterceptor) (interface{}, error) {
	in := new(HelloRequest)
	if err := dec(in); err != nil {
		return nil, err
	}
	if interceptor == nil {
		return srv.(GreeterServer).SayHello(ctx, in)
	}
	info := &grpc.UnaryServerInfo{
		Server:     srv,
		FullMethod: "/test.Greeter/SayHello",
	}
	handler := func(ctx context.Context, req interface{}) (interface{}, error) {
		return srv.(GreeterServer).SayHello(ctx, req.(*HelloRequest))
	}
	return interceptor(ctx, in, info, handler)
}

var StreamingService_ServiceDesc = grpc.ServiceDesc{
	ServiceName: "test.StreamingService",
	HandlerType: (*StreamingServiceServer)(nil),
	Methods:     []grpc.MethodDesc{},
	Streams: []grpc.StreamDesc{
		{
			StreamName:    "ServerStream",
			Handler:       _StreamingService_ServerStream_Handler,
			ServerStreams: true,
		},
		{
			StreamName:    "ClientStream",
			Handler:       _StreamingService_ClientStream_Handler,
			ClientStreams: true,
		},
		{
			StreamName:    "BidiStream",
			Handler:       _StreamingService_BidiStream_Handler,
			ServerStreams: true,
			ClientStreams: true,
		},
		{
			StreamName:    "ErrorStream",
			Handler:       _StreamingService_ErrorStream_Handler,
			ServerStreams: true,
		},
	},
	Metadata: "test.proto",
}

func _StreamingService_ServerStream_Handler(srv interface{}, stream grpc.ServerStream) error {
	m := new(StreamRequest)
	if err := stream.RecvMsg(m); err != nil {
		return err
	}
	return srv.(StreamingServiceServer).ServerStream(m, &streamingServiceServerStreamServer{stream})
}

type streamingServiceServerStreamServer struct {
	grpc.ServerStream
}

func (x *streamingServiceServerStreamServer) Send(m *StreamResponse) error {
	return x.ServerStream.SendMsg(m)
}

func _StreamingService_ClientStream_Handler(srv interface{}, stream grpc.ServerStream) error {
	return srv.(StreamingServiceServer).ClientStream(&streamingServiceClientStreamServer{stream})
}

type streamingServiceClientStreamServer struct {
	grpc.ServerStream
}

func (x *streamingServiceClientStreamServer) Recv() (*StreamRequest, error) {
	m := new(StreamRequest)
	if err := x.ServerStream.RecvMsg(m); err != nil {
		return nil, err
	}
	return m, nil
}

func (x *streamingServiceClientStreamServer) SendAndClose(m *StreamResponse) error {
	return x.ServerStream.SendMsg(m)
}

func _StreamingService_BidiStream_Handler(srv interface{}, stream grpc.ServerStream) error {
	return srv.(StreamingServiceServer).BidiStream(&streamingServiceBidiStreamServer{stream})
}

type streamingServiceBidiStreamServer struct {
	grpc.ServerStream
}

func (x *streamingServiceBidiStreamServer) Send(m *StreamResponse) error {
	return x.ServerStream.SendMsg(m)
}

func (x *streamingServiceBidiStreamServer) Recv() (*StreamRequest, error) {
	m := new(StreamRequest)
	if err := x.ServerStream.RecvMsg(m); err != nil {
		return nil, err
	}
	return m, nil
}

func _StreamingService_ErrorStream_Handler(srv interface{}, stream grpc.ServerStream) error {
	m := new(StreamRequest)
	if err := stream.RecvMsg(m); err != nil {
		return err
	}
	return srv.(StreamingServiceServer).ErrorStream(m, &streamingServiceErrorStreamServer{stream})
}

type streamingServiceErrorStreamServer struct {
	grpc.ServerStream
}

func (x *streamingServiceErrorStreamServer) Send(m *StreamResponse) error {
	return x.ServerStream.SendMsg(m)
}

// MockServer represents a mock gRPC server for testing
type MockServer struct {
	server *grpc.Server
	listener net.Listener
	port   int
	mu     sync.RWMutex
}

// NewMockServer creates a new mock gRPC server
func NewMockServer() *MockServer {
	return &MockServer{}
}

// Start starts the mock server on a random port
func (m *MockServer) Start() error {
	m.mu.Lock()
	defer m.mu.Unlock()

	lis, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		return fmt.Errorf("failed to listen: %w", err)
	}

	m.listener = lis
	m.port = lis.Addr().(*net.TCPAddr).Port
	m.server = grpc.NewServer()

	// Register services
	RegisterGreeterServer(m.server, &UnimplementedGreeterServer{})
	RegisterStreamingServiceServer(m.server, &UnimplementedStreamingServiceServer{})

	go func() {
		if err := m.server.Serve(lis); err != nil {
			fmt.Printf("Server error: %v\n", err)
		}
	}()

	return nil
}

// Stop stops the mock server
func (m *MockServer) Stop() {
	m.mu.Lock()
	defer m.mu.Unlock()

	if m.server != nil {
		m.server.Stop()
	}
	if m.listener != nil {
		m.listener.Close()
	}
}

// Address returns the server address
func (m *MockServer) Address() string {
	m.mu.RLock()
	defer m.mu.RUnlock()
	return fmt.Sprintf("127.0.0.1:%d", m.port)
}

// Port returns the server port
func (m *MockServer) Port() int {
	m.mu.RLock()
	defer m.mu.RUnlock()
	return m.port
}

// Ensure MockServer implements the interfaces
var _ emptypb.Empty = emptypb.Empty{}
var _ protoimpl.MessageState = protoimpl.MessageState{}
