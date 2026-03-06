package env

import (
	"encoding/json"
	"fmt"
	"os"
	"regexp"
	"strings"
)

var (
	// ${VAR} or ${VAR:-default} pattern
	envVarPattern = regexp.MustCompile(`\$\{([^}]+)\}`)
)

type Resolver struct {
	variables map[string]string
}

func NewResolver() *Resolver {
	return &Resolver{
		variables: make(map[string]string),
	}
}

func NewResolverWithEnv() *Resolver {
	r := NewResolver()
	for _, e := range os.Environ() {
		pair := strings.SplitN(e, "=", 2)
		if len(pair) == 2 {
			r.SetVariable(pair[0], pair[1])
		}
	}
	return r
}

func (r *Resolver) SetVariable(key, value string) {
	r.variables[key] = value
}

func (r *Resolver) GetVariable(key string) string {
	return r.variables[key]
}

func (r *Resolver) GetAllVariables() map[string]string {
	result := make(map[string]string, len(r.variables))
	for k, v := range r.variables {
		result[k] = v
	}
	return result
}

func (r *Resolver) DeleteVariable(key string) {
	delete(r.variables, key)
}

func (r *Resolver) Resolve(text string) string {
	return envVarPattern.ReplaceAllStringFunc(text, func(match string) string {
		content := match[2 : len(match)-1] // Remove ${ and }

		// Check for default value syntax: VAR:-default
		if idx := strings.Index(content, ":-"); idx != -1 {
			varName := content[:idx]
			defaultValue := content[idx+2:]
			if val, exists := r.variables[varName]; exists && val != "" {
				return val
			}
			return defaultValue
		}

		// Simple variable substitution
		if val, exists := r.variables[content]; exists {
			return val
		}

		// Return empty string if variable not found
		return ""
	})
}

func (r *Resolver) ResolveJSON(jsonStr string) (string, error) {
	var data interface{}
	if err := json.Unmarshal([]byte(jsonStr), &data); err != nil {
		return "", fmt.Errorf("invalid JSON: %w", err)
	}

	resolved := r.resolveValue(data)

	result, err := json.Marshal(resolved)
	if err != nil {
		return "", fmt.Errorf("failed to marshal resolved JSON: %w", err)
	}

	return string(result), nil
}

func (r *Resolver) resolveValue(v interface{}) interface{} {
	switch val := v.(type) {
	case string:
		return r.Resolve(val)
	case map[string]interface{}:
		result := make(map[string]interface{}, len(val))
		for k, v := range val {
			result[k] = r.resolveValue(v)
		}
		return result
	case []interface{}:
		result := make([]interface{}, len(val))
		for i, v := range val {
			result[i] = r.resolveValue(v)
		}
		return result
	default:
		return v
	}
}

func (r *Resolver) ResolveMap(data map[string]interface{}) map[string]interface{} {
	result := make(map[string]interface{}, len(data))
	for k, v := range data {
		result[k] = r.resolveValue(v)
	}
	return result
}

func (r *Resolver) ResolveStringMap(data map[string]string) map[string]string {
	result := make(map[string]string, len(data))
	for k, v := range data {
		result[k] = r.Resolve(v)
	}
	return result
}

func (r *Resolver) Clear() {
	r.variables = make(map[string]string)
}

func (r *Resolver) LoadFromMap(vars map[string]string) {
	for k, v := range vars {
		r.SetVariable(k, v)
	}
}

func (r *Resolver) LoadFromEnvironment(prefix string) {
	for _, e := range os.Environ() {
		pair := strings.SplitN(e, "=", 2)
		if len(pair) == 2 {
			key := pair[0]
			if prefix == "" || strings.HasPrefix(key, prefix) {
				r.SetVariable(key, pair[1])
			}
		}
	}
}
