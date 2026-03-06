package tls

import (
	"crypto/ecdsa"
	"crypto/elliptic"
	"crypto/rand"
	"crypto/tls"
	"crypto/x509"
	"crypto/x509/pkix"
	"encoding/pem"
	"fmt"
	"math/big"
	"os"
	"sync"
	"time"

	"github.com/jacky/grpcui/sidecar/internal/storage"
)

type Manager struct {
	certs map[string]*tls.Config
	mu    sync.RWMutex
}

func NewManager() *Manager {
	return &Manager{
		certs: make(map[string]*tls.Config),
	}
}

func (m *Manager) LoadConfig(config storage.TLSConfig) (*tls.Config, error) {
	if !config.Enabled {
		return nil, nil
	}

	if config.Insecure {
		return &tls.Config{
			InsecureSkipVerify: true,
		}, nil
	}

	if config.CAFile != "" || config.CertFile != "" || config.KeyFile != "" {
		return m.LoadFromFiles(config.CAFile, config.CertFile, config.KeyFile)
	}

	return &tls.Config{}, nil
}

func (m *Manager) LoadFromFiles(caFile, certFile, keyFile string) (*tls.Config, error) {
	tlsConfig := &tls.Config{}

	// Load CA certificate
	if caFile != "" {
		caCert, err := os.ReadFile(caFile)
		if err != nil {
			return nil, fmt.Errorf("failed to read CA file: %w", err)
		}

		caCertPool := x509.NewCertPool()
		if !caCertPool.AppendCertsFromPEM(caCert) {
			return nil, fmt.Errorf("failed to parse CA certificate")
		}
		tlsConfig.RootCAs = caCertPool
	}

	// Load client certificate
	if certFile != "" && keyFile != "" {
		cert, err := tls.LoadX509KeyPair(certFile, keyFile)
		if err != nil {
			return nil, fmt.Errorf("failed to load client certificate: %w", err)
		}
		tlsConfig.Certificates = []tls.Certificate{cert}
	}

	return tlsConfig, nil
}

func (m *Manager) CreateSelfSigned() (*tls.Config, error) {
	// Generate private key
	privateKey, err := ecdsa.GenerateKey(elliptic.P256(), rand.Reader)
	if err != nil {
		return nil, fmt.Errorf("failed to generate private key: %w", err)
	}

	// Create certificate template
	template := x509.Certificate{
		SerialNumber: big.NewInt(1),
		Subject: pkix.Name{
			Organization:  []string{"gRPC UI"},
			Country:       []string{"US"},
			Province:      []string{""},
			Locality:      []string{""},
			StreetAddress: []string{""},
			PostalCode:    []string{""},
		},
		NotBefore:             time.Now(),
		NotAfter:              time.Now().Add(365 * 24 * time.Hour),
		KeyUsage:              x509.KeyUsageKeyEncipherment | x509.KeyUsageDigitalSignature,
		ExtKeyUsage:           []x509.ExtKeyUsage{x509.ExtKeyUsageClientAuth, x509.ExtKeyUsageServerAuth},
		BasicConstraintsValid: true,
		DNSNames:              []string{"localhost"},
	}

	// Generate certificate
	certDER, err := x509.CreateCertificate(rand.Reader, &template, &template, &privateKey.PublicKey, privateKey)
	if err != nil {
		return nil, fmt.Errorf("failed to create certificate: %w", err)
	}

	// Encode certificate and private key
	certPEM := pem.EncodeToMemory(&pem.Block{Type: "CERTIFICATE", Bytes: certDER})

	keyDER, err := x509.MarshalECPrivateKey(privateKey)
	if err != nil {
		return nil, fmt.Errorf("failed to marshal private key: %w", err)
	}
	keyPEM := pem.EncodeToMemory(&pem.Block{Type: "EC PRIVATE KEY", Bytes: keyDER})

	// Load the certificate
	cert, err := tls.X509KeyPair(certPEM, keyPEM)
	if err != nil {
		return nil, fmt.Errorf("failed to load generated certificate: %w", err)
	}

	tlsConfig := &tls.Config{
		Certificates:       []tls.Certificate{cert},
		InsecureSkipVerify: true,
	}

	return tlsConfig, nil
}

func (m *Manager) ValidateCertificate(cert *x509.Certificate) error {
	if cert == nil {
		return fmt.Errorf("certificate is nil")
	}

	now := time.Now()
	if now.Before(cert.NotBefore) {
		return fmt.Errorf("certificate is not yet valid")
	}
	if now.After(cert.NotAfter) {
		return fmt.Errorf("certificate has expired")
	}

	return nil
}

func (m *Manager) ValidateCertificateFile(certFile string) error {
	data, err := os.ReadFile(certFile)
	if err != nil {
		return fmt.Errorf("failed to read certificate file: %w", err)
	}

	block, _ := pem.Decode(data)
	if block == nil {
		return fmt.Errorf("failed to decode PEM block")
	}

	cert, err := x509.ParseCertificate(block.Bytes)
	if err != nil {
		return fmt.Errorf("failed to parse certificate: %w", err)
	}

	return m.ValidateCertificate(cert)
}

func (m *Manager) GetCachedConfig(key string) (*tls.Config, bool) {
	m.mu.RLock()
	defer m.mu.RUnlock()
	config, exists := m.certs[key]
	return config, exists
}

func (m *Manager) CacheConfig(key string, config *tls.Config) {
	m.mu.Lock()
	defer m.mu.Unlock()
	m.certs[key] = config
}

func (m *Manager) ClearCache() {
	m.mu.Lock()
	defer m.mu.Unlock()
	m.certs = make(map[string]*tls.Config)
}

func (m *Manager) RemoveFromCache(key string) {
	m.mu.Lock()
	defer m.mu.Unlock()
	delete(m.certs, key)
}

func (m *Manager) GetServerName(config *tls.Config) string {
	if config == nil {
		return ""
	}
	return config.ServerName
}

func (m *Manager) SetServerName(config *tls.Config, serverName string) {
	if config != nil {
		config.ServerName = serverName
	}
}
