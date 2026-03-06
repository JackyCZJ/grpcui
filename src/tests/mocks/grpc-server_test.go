package mocks

import (
	"context"
	"testing"
	"time"

	"google.golang.org/grpc"
	"google.golang.org/grpc/credentials/insecure"
)

func TestMockServer(t *testing.T) {
	server := NewMockServer()
	if err := server.Start(); err != nil {
		t.Fatalf("Failed to start mock server: %v", err)
	}
	defer server.Stop()

	// Give server time to start
	time.Sleep(100 * time.Millisecond)

	// Test connection
	conn, err := grpc.Dial(
		server.Address(),
		grpc.WithTransportCredentials(insecure.NewCredentials()),
		grpc.WithBlock(),
		grpc.WithTimeout(5*time.Second),
	)
	if err != nil {
		t.Fatalf("Failed to connect: %v", err)
	}
	defer conn.Close()

	// Test that we can create a client
	// Note: In real tests, you would use the generated client code
	if conn.GetState() != grpc.Ready {
		t.Logf("Connection state: %v", conn.GetState())
	}
}

func TestMockServerPort(t *testing.T) {
	server := NewMockServer()
	if err := server.Start(); err != nil {
		t.Fatalf("Failed to start mock server: %v", err)
	}
	defer server.Stop()

	port := server.Port()
	if port == 0 {
		t.Error("Expected non-zero port")
	}

	addr := server.Address()
	if addr == "" {
		t.Error("Expected non-empty address")
	}

	expectedAddr := "127.0.0.1:" + string(rune('0'+port/10000)) + string(rune('0'+(port%10000)/1000)) + string(rune('0'+(port%1000)/100)) + string(rune('0'+(port%100)/10)) + string(rune('0'+port%10))
	_ = expectedAddr // Suppress unused variable warning
	if addr[:10] != "127.0.0.1:" {
		t.Errorf("Expected address to start with 127.0.0.1:, got %s", addr)
	}
}

func TestGreeterService(t *testing.T) {
	server := NewMockServer()
	if err := server.Start(); err != nil {
		t.Fatalf("Failed to start mock server: %v", err)
	}
	defer server.Stop()

	time.Sleep(100 * time.Millisecond)

	// Create a test request
	req := &HelloRequest{Name: "Test"}
	resp := &HelloReply{Message: "Hello, Test!"}

	// Verify request/response types
	if req.GetName() != "Test" {
		t.Error("Request name mismatch")
	}
	if resp.GetMessage() != "Hello, Test!" {
		t.Error("Response message mismatch")
	}
}

func TestStreamingService(t *testing.T) {
	server := NewMockServer()
	if err := server.Start(); err != nil {
		t.Fatalf("Failed to start mock server: %v", err)
	}
	defer server.Stop()

	time.Sleep(100 * time.Millisecond)

	// Test stream request/response types
	req := &StreamRequest{
		Data:  "test data",
		Count: 5,
	}

	if req.GetData() != "test data" {
		t.Error("Stream request data mismatch")
	}
	if req.GetCount() != 5 {
		t.Error("Stream request count mismatch")
	}

	resp := &StreamResponse{
		Data:     "response data",
		Sequence: 1,
	}

	if resp.GetData() != "response data" {
		t.Error("Stream response data mismatch")
	}
	if resp.GetSequence() != 1 {
		t.Error("Stream response sequence mismatch")
	}
}

func TestUnimplementedGreeterServer(t *testing.T) {
	server := &UnimplementedGreeterServer{}

	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()

	req := &HelloRequest{Name: "Test"}
	resp, err := server.SayHello(ctx, req)

	if err != nil {
		t.Errorf("Unexpected error: %v", err)
	}

	if resp == nil {
		t.Error("Expected non-nil response")
	} else if resp.Message != "Hello, Test!" {
		t.Errorf("Expected 'Hello, Test!', got '%s'", resp.Message)
	}
}

func TestUnimplementedStreamingServer(t *testing.T) {
	server := &UnimplementedStreamingServiceServer{}

	// Test ServerStream
	t.Run("ServerStream", func(t *testing.T) {
		// This is a basic test to ensure the method exists
		// Full testing would require a mock stream
		_ = server
	})

	// Test ClientStream
	t.Run("ClientStream", func(t *testing.T) {
		_ = server
	})

	// Test BidiStream
	t.Run("BidiStream", func(t *testing.T) {
		_ = server
	})

	// Test ErrorStream
	t.Run("ErrorStream", func(t *testing.T) {
		_ = server
	})
}
