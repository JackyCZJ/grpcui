package logger

import (
	"encoding/json"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"sync"
	"time"
)

// LogLevel represents the severity of a log message
type LogLevel int

const (
	DebugLevel LogLevel = iota
	InfoLevel
	WarnLevel
	ErrorLevel
)

func (l LogLevel) String() string {
	switch l {
	case DebugLevel:
		return "DEBUG"
	case InfoLevel:
		return "INFO"
	case WarnLevel:
		return "WARN"
	case ErrorLevel:
		return "ERROR"
	default:
		return "UNKNOWN"
	}
}

// LogEntry represents a structured log entry
type LogEntry struct {
	Timestamp string                 `json:"timestamp"`
	Level     string                 `json:"level"`
	Message   string                 `json:"message"`
	Fields    map[string]interface{} `json:"fields,omitempty"`
}

// Logger provides structured logging with multiple outputs
type Logger struct {
	level      LogLevel
	consoleOut io.Writer
	fileOut    io.Writer
	mu         sync.RWMutex
	filePath   string
}

var (
	defaultLogger *Logger
	once          sync.Once
)

// GetLogger returns the singleton logger instance
func GetLogger() *Logger {
	once.Do(func() {
		defaultLogger = NewLogger(InfoLevel)
	})
	return defaultLogger
}

// NewLogger creates a new logger instance
func NewLogger(level LogLevel) *Logger {
	return &Logger{
		level:      level,
		consoleOut: os.Stdout,
		fileOut:    nil,
		filePath:   "",
	}
}

// SetLevel sets the minimum log level
func (l *Logger) SetLevel(level LogLevel) {
	l.mu.Lock()
	defer l.mu.Unlock()
	l.level = level
}

// SetFileOutput sets the file output for logging
func (l *Logger) SetFileOutput(filePath string) error {
	l.mu.Lock()
	defer l.mu.Unlock()

	// Close existing file if any
	if l.fileOut != nil {
		if closer, ok := l.fileOut.(io.Closer); ok {
			if err := closer.Close(); err != nil {
				return fmt.Errorf("failed to close previous log file: %w", err)
			}
		}
	}

	// Create directory if it doesn't exist
	dir := filepath.Dir(filePath)
	if err := os.MkdirAll(dir, 0755); err != nil {
		return fmt.Errorf("failed to create log directory: %w", err)
	}

	// Open file
	file, err := os.OpenFile(filePath, os.O_CREATE|os.O_WRONLY|os.O_APPEND, 0644)
	if err != nil {
		return fmt.Errorf("failed to open log file: %w", err)
	}

	l.fileOut = file
	l.filePath = filePath
	return nil
}

// log writes a log entry
func (l *Logger) log(level LogLevel, message string, fields map[string]interface{}) {
	l.mu.RLock()
	minLevel := l.level
	l.mu.RUnlock()

	if level < minLevel {
		return
	}

	entry := LogEntry{
		Timestamp: time.Now().Format(time.RFC3339Nano),
		Level:     level.String(),
		Message:   message,
		Fields:    fields,
	}

	// Write to console (human-readable)
	consoleLine := fmt.Sprintf("[%s] %s: %s", entry.Timestamp, entry.Level, entry.Message)
	if len(fields) > 0 {
		fieldsJSON, _ := json.Marshal(fields)
		consoleLine += " " + string(fieldsJSON)
	}
	consoleLine += "\n"
	_, _ = fmt.Fprint(l.consoleOut, consoleLine)

	// Write to file (structured JSON)
	if l.fileOut != nil {
		jsonLine, err := json.Marshal(entry)
		if err == nil {
			jsonLine = append(jsonLine, '\n')
			_, _ = l.fileOut.Write(jsonLine)
		}
	}
}

// Debug logs a debug message
func (l *Logger) Debug(message string, fields ...map[string]interface{}) {
	var f map[string]interface{}
	if len(fields) > 0 {
		f = fields[0]
	}
	l.log(DebugLevel, message, f)
}

// Info logs an info message
func (l *Logger) Info(message string, fields ...map[string]interface{}) {
	var f map[string]interface{}
	if len(fields) > 0 {
		f = fields[0]
	}
	l.log(InfoLevel, message, f)
}

// Warn logs a warning message
func (l *Logger) Warn(message string, fields ...map[string]interface{}) {
	var f map[string]interface{}
	if len(fields) > 0 {
		f = fields[0]
	}
	l.log(WarnLevel, message, f)
}

// Error logs an error message
func (l *Logger) Error(message string, fields ...map[string]interface{}) {
	var f map[string]interface{}
	if len(fields) > 0 {
		f = fields[0]
	}
	l.log(ErrorLevel, message, f)
}

// Errorf logs a formatted error message
func (l *Logger) Errorf(format string, args ...interface{}) {
	l.log(ErrorLevel, fmt.Sprintf(format, args...), nil)
}

// Infof logs a formatted info message
func (l *Logger) Infof(format string, args ...interface{}) {
	l.log(InfoLevel, fmt.Sprintf(format, args...), nil)
}

// Debugf logs a formatted debug message
func (l *Logger) Debugf(format string, args ...interface{}) {
	l.log(DebugLevel, fmt.Sprintf(format, args...), nil)
}

// Warnf logs a formatted warning message
func (l *Logger) Warnf(format string, args ...interface{}) {
	l.log(WarnLevel, fmt.Sprintf(format, args...), nil)
}

// Close closes the logger and its file output
func (l *Logger) Close() error {
	l.mu.Lock()
	defer l.mu.Unlock()

	if l.fileOut != nil {
		if closer, ok := l.fileOut.(io.Closer); ok {
			return closer.Close()
		}
	}
	return nil
}

// Convenience functions for the default logger

func Debug(message string, fields ...map[string]interface{}) {
	GetLogger().Debug(message, fields...)
}

func Info(message string, fields ...map[string]interface{}) {
	GetLogger().Info(message, fields...)
}

func Warn(message string, fields ...map[string]interface{}) {
	GetLogger().Warn(message, fields...)
}

func Error(message string, fields ...map[string]interface{}) {
	GetLogger().Error(message, fields...)
}

func Debugf(format string, args ...interface{}) {
	GetLogger().Debugf(format, args...)
}

func Infof(format string, args ...interface{}) {
	GetLogger().Infof(format, args...)
}

func Warnf(format string, args ...interface{}) {
	GetLogger().Warnf(format, args...)
}

func Errorf(format string, args ...interface{}) {
	GetLogger().Errorf(format, args...)
}

func SetLevel(level LogLevel) {
	GetLogger().SetLevel(level)
}

func SetFileOutput(filePath string) error {
	return GetLogger().SetFileOutput(filePath)
}
