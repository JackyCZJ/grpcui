package storage

import "time"

// Project represents a gRPC UI project
type Project struct {
	ID                   string    `json:"id"`
	Name                 string    `json:"name"`
	Description          string    `json:"description"`
	DefaultEnvironmentID string    `json:"default_environment_id,omitempty"`
	ProtoFiles           []string  `json:"proto_files,omitempty"`
	CreatedAt            time.Time `json:"created_at"`
	UpdatedAt            time.Time `json:"updated_at"`
}

type Environment struct {
	ID        string            `json:"id"`
	ProjectID string            `json:"project_id"`
	Name      string            `json:"name"`
	BaseURL   string            `json:"base_url"`
	Variables map[string]string `json:"variables"`
	Headers   map[string]string `json:"headers"`
	TLSConfig *TLSConfig        `json:"tls_config,omitempty"`
	IsDefault bool              `json:"is_default"`
	CreatedAt time.Time         `json:"created_at"`
	UpdatedAt time.Time         `json:"updated_at"`
}

type TLSConfig struct {
	Enabled    bool   `json:"enabled"`
	CAFile     string `json:"ca_file,omitempty"`
	CertFile   string `json:"cert_file,omitempty"`
	KeyFile    string `json:"key_file,omitempty"`
	ServerName string `json:"server_name,omitempty"`
	Insecure   bool   `json:"insecure"`
}

type Variable struct {
	Key    string `json:"key"`
	Value  string `json:"value"`
	Secret bool   `json:"secret"`
}

type Collection struct {
	ID        string        `json:"id"`
	ProjectID string        `json:"project_id"`
	Name      string        `json:"name"`
	Folders   []Folder      `json:"folders"`
	Items     []RequestItem `json:"items"`
	CreatedAt time.Time     `json:"created_at"`
	UpdatedAt time.Time     `json:"updated_at"`
}

type Folder struct {
	ID    string        `json:"id"`
	Name  string        `json:"name"`
	Items []RequestItem `json:"items"`
}

type RequestItem struct {
	ID            string            `json:"id"`
	Name          string            `json:"name"`
	Type          string            `json:"type"`
	Service       string            `json:"service"`
	Method        string            `json:"method"`
	Body          string            `json:"body"`
	Metadata      map[string]string `json:"metadata"`
	EnvRefType    string            `json:"env_ref_type"`
	EnvironmentID string            `json:"environment_id,omitempty"`
}

type History struct {
	ID              string      `json:"id"`
	ProjectID       string      `json:"project_id,omitempty"`
	Timestamp       int64       `json:"timestamp"`
	Service         string      `json:"service"`
	Method          string      `json:"method"`
	Address         string      `json:"address"`
	Status          string      `json:"status"`
	Duration        int64       `json:"duration"`
	RequestSnapshot RequestItem `json:"request_snapshot"`
}

type HistoryEntry struct {
	ID              string      `json:"id"`
	ProjectID       string      `json:"project_id,omitempty"`
	Timestamp       int64       `json:"timestamp"`
	Service         string      `json:"service"`
	Method          string      `json:"method"`
	Address         string      `json:"address"`
	Status          string      `json:"status"`
	Duration        int64       `json:"duration"`
	RequestSnapshot RequestItem `json:"request_snapshot"`
}

type Filters struct {
	Service   string
	Method    string
	Status    string
	StartTime int64
	EndTime   int64
	Limit     int
	Offset    int
}
