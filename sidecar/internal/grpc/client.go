package grpc

import (
	"context"
	"crypto/tls"
	"fmt"
	"io"
	"time"

	"google.golang.org/grpc"
	"google.golang.org/grpc/credentials"
	"google.golang.org/grpc/credentials/insecure"
	"google.golang.org/grpc/metadata"
	"google.golang.org/protobuf/types/dynamicpb"
)

// DialOption is a function that configures dial options
type DialOption func(*dialConfig)

type dialConfig struct {
	tlsConfig *tls.Config
	insecure  bool
	timeout   time.Duration
}

// WithTLS configures TLS for the connection
func WithTLS(config *tls.Config) DialOption {
	return func(c *dialConfig) {
		c.tlsConfig = config
	}
}

// WithInsecure allows insecure connections
func WithInsecure() DialOption {
	return func(c *dialConfig) {
		c.insecure = true
	}
}

// WithTimeout sets the dial timeout
func WithTimeout(timeout time.Duration) DialOption {
	return func(c *dialConfig) {
		c.timeout = timeout
	}
}

// Client manages gRPC connections and invocations
type Client struct {
	conn    *grpc.ClientConn
	address string
	config  *dialConfig
}

// Connect establishes a connection to a gRPC server
func Connect(address string, opts ...DialOption) (*Client, error) {
	config := &dialConfig{
		timeout: 10 * time.Second,
	}

	for _, opt := range opts {
		opt(config)
	}

	dialOpts := []grpc.DialOption{}

	// Configure credentials
	if config.insecure {
		dialOpts = append(dialOpts, grpc.WithTransportCredentials(insecure.NewCredentials()))
	} else if config.tlsConfig != nil {
		dialOpts = append(dialOpts, grpc.WithTransportCredentials(credentials.NewTLS(config.tlsConfig)))
	} else {
		dialOpts = append(dialOpts, grpc.WithTransportCredentials(credentials.NewTLS(&tls.Config{})))
	}

	ctx, cancel := context.WithTimeout(context.Background(), config.timeout)
	defer cancel()

	//nolint:staticcheck // 这里依赖 DialContext 的超时语义，后续统一迁移到 NewClient。
	conn, err := grpc.DialContext(ctx, address, dialOpts...)
	if err != nil {
		return nil, fmt.Errorf("failed to connect to %s: %w", address, err)
	}

	return &Client{
		conn:    conn,
		address: address,
		config:  config,
	}, nil
}

// Invoke performs a unary gRPC call
func (c *Client) Invoke(ctx context.Context, service, method string, message *dynamicpb.Message, md metadata.MD) (*dynamicpb.Message, metadata.MD, error) {
	fullMethod := fmt.Sprintf("/%s/%s", service, method)

	// Create an empty response message
	resp := dynamicpb.NewMessage(message.Descriptor())

	var header metadata.MD
	opts := []grpc.CallOption{grpc.Header(&header)}

	if md != nil {
		ctx = metadata.NewOutgoingContext(ctx, md)
	}

	err := c.conn.Invoke(ctx, fullMethod, message, resp, opts...)
	if err != nil {
		return nil, nil, fmt.Errorf("invoke failed: %w", err)
	}

	return resp, header, nil
}

// Stream represents a gRPC streaming connection
type Stream struct {
	grpc.ClientStream
	desc *grpc.StreamDesc
}

// Send sends a message on the stream
func (s *Stream) Send(msg *dynamicpb.Message) error {
	return s.SendMsg(msg)
}

// Recv receives a message from the stream
func (s *Stream) Recv() (*dynamicpb.Message, error) {
	// We need to know the message type - this is handled by the caller
	// The caller should provide a message to unmarshal into
	return nil, fmt.Errorf("use RecvInto with a message type")
}

// RecvInto receives a message into the provided message
func (s *Stream) RecvInto(msg *dynamicpb.Message) error {
	return s.RecvMsg(msg)
}

// InvokeServerStream initiates a server streaming call
func (c *Client) InvokeServerStream(ctx context.Context, service, method string, message *dynamicpb.Message, md metadata.MD) (*Stream, error) {
	fullMethod := fmt.Sprintf("/%s/%s", service, method)

	desc := &grpc.StreamDesc{
		StreamName:    method,
		ServerStreams: true,
	}

	if md != nil {
		ctx = metadata.NewOutgoingContext(ctx, md)
	}

	stream, err := c.conn.NewStream(ctx, desc, fullMethod)
	if err != nil {
		return nil, fmt.Errorf("failed to create stream: %w", err)
	}

	if err := stream.SendMsg(message); err != nil {
		return nil, fmt.Errorf("failed to send message: %w", err)
	}

	if err := stream.CloseSend(); err != nil {
		return nil, fmt.Errorf("failed to close send: %w", err)
	}

	return &Stream{ClientStream: stream, desc: desc}, nil
}

// InvokeClientStream initiates a client streaming call
func (c *Client) InvokeClientStream(ctx context.Context, service, method string, md metadata.MD) (*Stream, error) {
	fullMethod := fmt.Sprintf("/%s/%s", service, method)

	desc := &grpc.StreamDesc{
		StreamName:    method,
		ClientStreams: true,
	}

	if md != nil {
		ctx = metadata.NewOutgoingContext(ctx, md)
	}

	stream, err := c.conn.NewStream(ctx, desc, fullMethod)
	if err != nil {
		return nil, fmt.Errorf("failed to create stream: %w", err)
	}

	return &Stream{ClientStream: stream, desc: desc}, nil
}

// InvokeBidiStream initiates a bidirectional streaming call
func (c *Client) InvokeBidiStream(ctx context.Context, service, method string, md metadata.MD) (*Stream, error) {
	fullMethod := fmt.Sprintf("/%s/%s", service, method)

	desc := &grpc.StreamDesc{
		StreamName:    method,
		ServerStreams: true,
		ClientStreams: true,
	}

	if md != nil {
		ctx = metadata.NewOutgoingContext(ctx, md)
	}

	stream, err := c.conn.NewStream(ctx, desc, fullMethod)
	if err != nil {
		return nil, fmt.Errorf("failed to create stream: %w", err)
	}

	return &Stream{ClientStream: stream, desc: desc}, nil
}

// Close closes the gRPC connection
func (c *Client) Close() error {
	if c.conn != nil {
		return c.conn.Close()
	}
	return nil
}

// IsConnected returns true if the connection is ready
func (c *Client) IsConnected() bool {
	if c.conn == nil {
		return false
	}
	state := c.conn.GetState()
	return state.String() == "READY"
}

// Address returns the connection address
func (c *Client) Address() string {
	return c.address
}

// Connection returns the underlying gRPC connection
func (c *Client) Connection() *grpc.ClientConn {
	return c.conn
}

// StreamReader provides a convenient interface for reading from streams
type StreamReader struct {
	stream    *Stream
	msgType   func() *dynamicpb.Message
	cancelCtx context.CancelFunc
}

// NewStreamReader creates a new stream reader
func NewStreamReader(stream *Stream, msgType func() *dynamicpb.Message, cancel context.CancelFunc) *StreamReader {
	return &StreamReader{
		stream:    stream,
		msgType:   msgType,
		cancelCtx: cancel,
	}
}

// Recv reads the next message from the stream
func (r *StreamReader) Recv() (*dynamicpb.Message, error) {
	msg := r.msgType()
	if err := r.stream.RecvInto(msg); err != nil {
		if err == io.EOF {
			return nil, err
		}
		return nil, err
	}
	return msg, nil
}

// Close closes the stream
func (r *StreamReader) Close() error {
	if r.cancelCtx != nil {
		r.cancelCtx()
	}
	return r.stream.CloseSend()
}
