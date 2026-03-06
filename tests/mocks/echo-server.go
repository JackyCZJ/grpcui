package main

import (
	"context"
	"fmt"
	"io"
	"log"
	"net"
	"time"

	"google.golang.org/grpc"
	"google.golang.org/grpc/reflection"
)

// EchoRequest represents the request message
type EchoRequest struct {
	Message string `protobuf:"bytes,1,opt,name=message,proto3" json:"message,omitempty"`
	Count   int32  `protobuf:"varint,2,opt,name=count,proto3" json:"count,omitempty"`
}

func (x *EchoRequest) Reset()         { *x = EchoRequest{} }
func (x *EchoRequest) String() string { return fmt.Sprintf("EchoRequest{Message: %s, Count: %d}", x.Message, x.Count) }
func (*EchoRequest) ProtoMessage()    {}
func (x *EchoRequest) GetMessage() string {
	if x != nil {
		return x.Message
	}
	return ""
}
func (x *EchoRequest) GetCount() int32 {
	if x != nil {
		return x.Count
	}
	return 0
}

// EchoResponse represents the response message
type EchoResponse struct {
	Message   string `protobuf:"bytes,1,opt,name=message,proto3" json:"message,omitempty"`
	Sequence  int32  `protobuf:"varint,2,opt,name=sequence,proto3" json:"sequence,omitempty"`
	Timestamp int64  `protobuf:"varint,3,opt,name=timestamp,proto3" json:"timestamp,omitempty"`
}

func (x *EchoResponse) Reset()         { *x = EchoResponse{} }
func (x *EchoResponse) String() string { return fmt.Sprintf("EchoResponse{Message: %s, Sequence: %d, Timestamp: %d}", x.Message, x.Sequence, x.Timestamp) }
func (*EchoResponse) ProtoMessage()    {}
func (x *EchoResponse) GetMessage() string {
	if x != nil {
		return x.Message
	}
	return ""
}
func (x *EchoResponse) GetSequence() int32 {
	if x != nil {
		return x.Sequence
	}
	return 0
}
func (x *EchoResponse) GetTimestamp() int64 {
	if x != nil {
		return x.Timestamp
	}
	return 0
}

// EchoServiceServer is the server interface
type EchoServiceServer interface {
	UnaryEcho(context.Context, *EchoRequest) (*EchoResponse, error)
	ServerStreamingEcho(*EchoRequest, EchoService_ServerStreamingEchoServer) error
	ClientStreamingEcho(EchoService_ClientStreamingEchoServer) error
	BidiStreamingEcho(EchoService_BidiStreamingEchoServer) error
}

// Server stream interfaces
type EchoService_ServerStreamingEchoServer interface {
	Send(*EchoResponse) error
	grpc.ServerStream
}

type echoServiceServerStreamingEchoServer struct {
	grpc.ServerStream
}

func (x *echoServiceServerStreamingEchoServer) Send(m *EchoResponse) error {
	return x.ServerStream.SendMsg(m)
}

// Client stream interfaces
type EchoService_ClientStreamingEchoServer interface {
	Recv() (*EchoRequest, error)
	SendAndClose(*EchoResponse) error
	grpc.ServerStream
}

type echoServiceClientStreamingEchoServer struct {
	grpc.ServerStream
}

func (x *echoServiceClientStreamingEchoServer) Recv() (*EchoRequest, error) {
	m := new(EchoRequest)
	if err := x.ServerStream.RecvMsg(m); err != nil {
		return nil, err
	}
	return m, nil
}

func (x *echoServiceClientStreamingEchoServer) SendAndClose(m *EchoResponse) error {
	return x.ServerStream.SendMsg(m)
}

// Bidi stream interfaces
type EchoService_BidiStreamingEchoServer interface {
	Send(*EchoResponse) error
	Recv() (*EchoRequest, error)
	grpc.ServerStream
}

type echoServiceBidiStreamingEchoServer struct {
	grpc.ServerStream
}

func (x *echoServiceBidiStreamingEchoServer) Send(m *EchoResponse) error {
	return x.ServerStream.SendMsg(m)
}

func (x *echoServiceBidiStreamingEchoServer) Recv() (*EchoRequest, error) {
	m := new(EchoRequest)
	if err := x.ServerStream.RecvMsg(m); err != nil {
		return nil, err
	}
	return m, nil
}

// EchoServer implements EchoServiceServer
type EchoServer struct {
	UnimplementedEchoServiceServer
}

type UnimplementedEchoServiceServer struct{}

func (s *UnimplementedEchoServiceServer) UnaryEcho(ctx context.Context, req *EchoRequest) (*EchoResponse, error) {
	return &EchoResponse{
		Message:   req.Message,
		Sequence:  1,
		Timestamp: time.Now().Unix(),
	}, nil
}

func (s *UnimplementedEchoServiceServer) ServerStreamingEcho(req *EchoRequest, stream EchoService_ServerStreamingEchoServer) error {
	count := req.Count
	if count <= 0 {
		count = 3
	}
	for i := int32(0); i < count; i++ {
		if err := stream.Send(&EchoResponse{
			Message:   req.Message,
			Sequence:  i + 1,
			Timestamp: time.Now().Unix(),
		}); err != nil {
			return err
		}
		time.Sleep(100 * time.Millisecond)
	}
	return nil
}

func (s *UnimplementedEchoServiceServer) ClientStreamingEcho(stream EchoService_ClientStreamingEchoServer) error {
	var messages []string
	var count int32
	for {
		req, err := stream.Recv()
		if err == io.EOF {
			break
		}
		if err != nil {
			return err
		}
		messages = append(messages, req.Message)
		count++
	}
	var combined string
	for i, m := range messages {
		if i > 0 {
			combined += " | "
		}
		combined += m
	}
	return stream.SendAndClose(&EchoResponse{
		Message:   combined,
		Sequence:  count,
		Timestamp: time.Now().Unix(),
	})
}

func (s *UnimplementedEchoServiceServer) BidiStreamingEcho(stream EchoService_BidiStreamingEchoServer) error {
	var sequence int32
	for {
		req, err := stream.Recv()
		if err == io.EOF {
			return nil
		}
		if err != nil {
			return err
		}
		sequence++
		if err := stream.Send(&EchoResponse{
			Message:   req.Message,
			Sequence:  sequence,
			Timestamp: time.Now().Unix(),
		}); err != nil {
			return err
		}
	}
}

// Register function
func RegisterEchoServiceServer(s *grpc.Server, srv EchoServiceServer) {
	s.RegisterService(&EchoService_ServiceDesc, srv)
}

// Service descriptor
var EchoService_ServiceDesc = grpc.ServiceDesc{
	ServiceName: "echo.EchoService",
	HandlerType: (*EchoServiceServer)(nil),
	Methods: []grpc.MethodDesc{
		{
			MethodName: "UnaryEcho",
			Handler:    _EchoService_UnaryEcho_Handler,
		},
	},
	Streams: []grpc.StreamDesc{
		{
			StreamName:    "ServerStreamingEcho",
			Handler:       _EchoService_ServerStreamingEcho_Handler,
			ServerStreams: true,
		},
		{
			StreamName:    "ClientStreamingEcho",
			Handler:       _EchoService_ClientStreamingEcho_Handler,
			ClientStreams: true,
		},
		{
			StreamName:    "BidiStreamingEcho",
			Handler:       _EchoService_BidiStreamingEcho_Handler,
			ServerStreams: true,
			ClientStreams: true,
		},
	},
	Metadata: "echo.proto",
}

func _EchoService_UnaryEcho_Handler(srv interface{}, ctx context.Context, dec func(interface{}) error, interceptor grpc.UnaryServerInterceptor) (interface{}, error) {
	in := new(EchoRequest)
	if err := dec(in); err != nil {
		return nil, err
	}
	if interceptor == nil {
		return srv.(EchoServiceServer).UnaryEcho(ctx, in)
	}
	info := &grpc.UnaryServerInfo{
		Server:     srv,
		FullMethod: "/echo.EchoService/UnaryEcho",
	}
	handler := func(ctx context.Context, req interface{}) (interface{}, error) {
		return srv.(EchoServiceServer).UnaryEcho(ctx, req.(*EchoRequest))
	}
	return interceptor(ctx, in, info, handler)
}

func _EchoService_ServerStreamingEcho_Handler(srv interface{}, stream grpc.ServerStream) error {
	m := new(EchoRequest)
	if err := stream.RecvMsg(m); err != nil {
		return err
	}
	return srv.(EchoServiceServer).ServerStreamingEcho(m, &echoServiceServerStreamingEchoServer{stream})
}

func _EchoService_ClientStreamingEcho_Handler(srv interface{}, stream grpc.ServerStream) error {
	return srv.(EchoServiceServer).ClientStreamingEcho(&echoServiceClientStreamingEchoServer{stream})
}

func _EchoService_BidiStreamingEcho_Handler(srv interface{}, stream grpc.ServerStream) error {
	return srv.(EchoServiceServer).BidiStreamingEcho(&echoServiceBidiStreamingEchoServer{stream})
}

func main() {
	lis, err := net.Listen("tcp", "localhost:50051")
	if err != nil {
		log.Fatalf("Failed to listen: %v", err)
	}

	s := grpc.NewServer()
	RegisterEchoServiceServer(s, &EchoServer{})

	// Enable reflection for grpcurl
	reflection.Register(s)

	log.Printf("Echo gRPC server starting on %s", lis.Addr())
	log.Println("Services:")
	log.Println("  - echo.EchoService/UnaryEcho")
	log.Println("  - echo.EchoService/ServerStreamingEcho")
	log.Println("  - echo.EchoService/ClientStreamingEcho")
	log.Println("  - echo.EchoService/BidiStreamingEcho")
	log.Println("")
	log.Println("Test with grpcurl:")
	log.Println("  grpcurl -plaintext localhost:50051 list")
	log.Println("  grpcurl -plaintext localhost:50051 describe echo.EchoService")
	log.Println("  grpcurl -plaintext -d '{\"message\":\"hello\",\"count\":3}' localhost:50051 echo.EchoService/UnaryEcho")

	if err := s.Serve(lis); err != nil {
		log.Fatalf("Failed to serve: %v", err)
	}
}
