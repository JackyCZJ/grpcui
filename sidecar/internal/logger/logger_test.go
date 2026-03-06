package logger

import (
	"bytes"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestLogLevelString(t *testing.T) {
	tests := []struct {
		level    LogLevel
		expected string
	}{
		{DebugLevel, "DEBUG"},
		{InfoLevel, "INFO"},
		{WarnLevel, "WARN"},
		{ErrorLevel, "ERROR"},
		{LogLevel(99), "UNKNOWN"},
	}

	for _, tt := range tests {
		t.Run(tt.expected, func(t *testing.T) {
			if got := tt.level.String(); got != tt.expected {
				t.Errorf("LogLevel.String() = %v, want %v", got, tt.expected)
			}
		})
	}
}

func TestNewLogger(t *testing.T) {
	logger := NewLogger(InfoLevel)
	if logger == nil {
		t.Fatal("NewLogger returned nil")
	}
	if logger.level != InfoLevel {
		t.Errorf("Expected level %v, got %v", InfoLevel, logger.level)
	}
}

func TestLoggerSetLevel(t *testing.T) {
	logger := NewLogger(InfoLevel)
	logger.SetLevel(DebugLevel)
	if logger.level != DebugLevel {
		t.Errorf("Expected level %v, got %v", DebugLevel, logger.level)
	}
}

func TestLoggerLogging(t *testing.T) {
	var buf bytes.Buffer
	logger := NewLogger(DebugLevel)
	logger.consoleOut = &buf

	logger.Debug("debug message")
	logger.Info("info message")
	logger.Warn("warn message")
	logger.Error("error message")

	output := buf.String()

	if !strings.Contains(output, "DEBUG") || !strings.Contains(output, "debug message") {
		t.Error("Debug log not found")
	}
	if !strings.Contains(output, "INFO") || !strings.Contains(output, "info message") {
		t.Error("Info log not found")
	}
	if !strings.Contains(output, "WARN") || !strings.Contains(output, "warn message") {
		t.Error("Warn log not found")
	}
	if !strings.Contains(output, "ERROR") || !strings.Contains(output, "error message") {
		t.Error("Error log not found")
	}
}

func TestLoggerLevelFiltering(t *testing.T) {
	var buf bytes.Buffer
	logger := NewLogger(WarnLevel)
	logger.consoleOut = &buf

	logger.Debug("debug message")
	logger.Info("info message")
	logger.Warn("warn message")
	logger.Error("error message")

	output := buf.String()

	if strings.Contains(output, "DEBUG") {
		t.Error("Debug log should be filtered")
	}
	if strings.Contains(output, "INFO") {
		t.Error("Info log should be filtered")
	}
	if !strings.Contains(output, "WARN") {
		t.Error("Warn log should not be filtered")
	}
	if !strings.Contains(output, "ERROR") {
		t.Error("Error log should not be filtered")
	}
}

func TestLoggerWithFields(t *testing.T) {
	var buf bytes.Buffer
	logger := NewLogger(DebugLevel)
	logger.consoleOut = &buf

	fields := map[string]interface{}{
		"key1": "value1",
		"key2": 42,
	}
	logger.Info("message with fields", fields)

	output := buf.String()
	if !strings.Contains(output, "key1") || !strings.Contains(output, "value1") {
		t.Error("Fields not logged correctly")
	}
}

func TestLoggerFormattedMethods(t *testing.T) {
	var buf bytes.Buffer
	logger := NewLogger(DebugLevel)
	logger.consoleOut = &buf

	logger.Debugf("debug %s", "formatted")
	logger.Infof("info %s", "formatted")
	logger.Warnf("warn %s", "formatted")
	logger.Errorf("error %s", "formatted")

	output := buf.String()

	if !strings.Contains(output, "debug formatted") {
		t.Error("Debugf not working")
	}
	if !strings.Contains(output, "info formatted") {
		t.Error("Infof not working")
	}
	if !strings.Contains(output, "warn formatted") {
		t.Error("Warnf not working")
	}
	if !strings.Contains(output, "error formatted") {
		t.Error("Errorf not working")
	}
}

func TestLoggerFileOutput(t *testing.T) {
	tmpDir := t.TempDir()
	logFile := filepath.Join(tmpDir, "test.log")

	logger := NewLogger(InfoLevel)
	if err := logger.SetFileOutput(logFile); err != nil {
		t.Fatalf("Failed to set file output: %v", err)
	}
	defer func() {
		_ = logger.Close()
	}()

	logger.Info("test message")

	// Read log file
	content, err := os.ReadFile(logFile)
	if err != nil {
		t.Fatalf("Failed to read log file: %v", err)
	}

	if !strings.Contains(string(content), "test message") {
		t.Error("Message not written to file")
	}
	if !strings.Contains(string(content), `"level":"INFO"`) {
		t.Error("JSON format not correct")
	}
}

func TestGetLogger(t *testing.T) {
	logger1 := GetLogger()
	logger2 := GetLogger()

	if logger1 != logger2 {
		t.Error("GetLogger should return the same instance")
	}
}

func TestGlobalFunctions(t *testing.T) {
	// Create a buffer to capture output
	var buf bytes.Buffer
	logger := NewLogger(DebugLevel)
	logger.consoleOut = &buf

	// Replace the default logger
	defaultLogger = logger

	Debug("debug")
	Info("info")
	Warn("warn")
	Error("error")

	Debugf("%s", "debugf")
	Infof("%s", "infof")
	Warnf("%s", "warnf")
	Errorf("%s", "errorf")

	output := buf.String()

	expected := []string{"debug", "info", "warn", "error", "debugf", "infof", "warnf", "errorf"}
	for _, exp := range expected {
		if !strings.Contains(output, exp) {
			t.Errorf("Expected %q in output", exp)
		}
	}
}

func TestSetLevel(t *testing.T) {
	logger := NewLogger(DebugLevel)
	defaultLogger = logger

	SetLevel(ErrorLevel)
	if defaultLogger.level != ErrorLevel {
		t.Error("SetLevel did not work")
	}
}

func TestLoggerClose(t *testing.T) {
	tmpDir := t.TempDir()
	logFile := filepath.Join(tmpDir, "test.log")

	logger := NewLogger(InfoLevel)
	if err := logger.SetFileOutput(logFile); err != nil {
		t.Fatalf("Failed to set file output: %v", err)
	}

	if err := logger.Close(); err != nil {
		t.Errorf("Close failed: %v", err)
	}
}
