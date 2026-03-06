//go:build cgo

package main

import (
	"encoding/json"
	"fmt"
	"sync"

	"github.com/jacky/grpcui/sidecar/internal/proto"
	"google.golang.org/grpc"
)

// Bridge is the main handle for FFI operations
type Bridge struct {
	parser *proto.Parser
	conn   *grpc.ClientConn
	mu     sync.RWMutex
}

// TLSConfig represents TLS configuration for reflection connections
type TLSConfig struct {
	Insecure bool   `json:"insecure"`
	CertPath string `json:"cert_path,omitempty"`
	KeyPath  string `json:"key_path,omitempty"`
	CAPath   string `json:"ca_path,omitempty"`
}

// ErrorResponse represents an error response
type ErrorResponse struct {
	Error string `json:"error"`
}

// ServiceListResponse represents the list of services
type ServiceListResponse struct {
	Services []proto.ServiceInfo `json:"services"`
}

// MethodListResponse represents the list of methods
type MethodListResponse struct {
	Methods []proto.MethodInfo `json:"methods"`
}

// EncodeResponse represents the encoded wire format response
type EncodeResponse struct {
	Data []byte `json:"data"`
}

// DecodeResponse represents the decoded JSON response
type DecodeResponse struct {
	JSON string `json:"json"`
}

// Global bridge store for handle management
type bridgeStore struct {
	bridges map[uintptr]*Bridge
	nextID  uintptr
	mu      sync.RWMutex
}

var store = &bridgeStore{
	bridges: make(map[uintptr]*Bridge),
}

func (s *bridgeStore) add(bridge *Bridge) uintptr {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.nextID++
	id := s.nextID
	s.bridges[id] = bridge
	return id
}

func (s *bridgeStore) get(id uintptr) (*Bridge, bool) {
	s.mu.RLock()
	defer s.mu.RUnlock()
	bridge, ok := s.bridges[id]
	return bridge, ok
}

func (s *bridgeStore) remove(id uintptr) {
	s.mu.Lock()
	defer s.mu.Unlock()
	delete(s.bridges, id)
}

// Global error tracking for last_error() function
var (
	lastError   string
	lastErrorMu sync.RWMutex
)

func setLastError(err error) {
	lastErrorMu.Lock()
	defer lastErrorMu.Unlock()
	if err != nil {
		lastError = err.Error()
	} else {
		lastError = ""
	}
}

func getLastError() string {
	lastErrorMu.RLock()
	defer lastErrorMu.RUnlock()
	return lastError
}

// toJSONString marshals a value to JSON string
func toJSONString(v interface{}) string {
	data, err := json.Marshal(v)
	if err != nil {
		return fmt.Sprintf(`{"error": "failed to marshal: %s"}`, err.Error())
	}
	return string(data)
}
