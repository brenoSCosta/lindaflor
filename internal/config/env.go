// Package config reads every environment variable the application uses.
//
// Every key may also be supplied Docker-secret style as `KEY_FILE` (a path
// whose contents are the value, trailing newlines trimmed). The direct
// variable wins, and empty values count as unset. The accessor functions
// re-read the environment on every call (so tests can use t.Setenv and key
// rotation needs no restart); [Load] snapshots everything the web process
// needs into a [Config] and validates production requirements.
package config

import (
	"os"
	"strconv"
	"strings"
)

// ReadEnvOrFile reads KEY, falling back to the file named by KEY_FILE.
// Direct env wins, empty values are treated as unset, trailing CR/LF of the
// file contents are trimmed.
func ReadEnvOrFile(key string) (string, bool) {
	if v := os.Getenv(key); v != "" {
		return v, true
	}
	path := os.Getenv(key + "_FILE")
	if path == "" {
		return "", false
	}
	raw, err := os.ReadFile(path) //nolint:gosec // operator-supplied secret path
	if err != nil {
		return "", false
	}
	v := strings.TrimRight(string(raw), "\r\n")
	if v == "" {
		return "", false
	}
	return v, true
}

// envNonEmpty is ReadEnvOrFile returning "" when unset.
func envNonEmpty(key string) string {
	v, _ := ReadEnvOrFile(key)
	return v
}

func envParseInt(key string, def int64) int64 {
	v, ok := ReadEnvOrFile(key)
	if !ok {
		return def
	}
	n, err := strconv.ParseInt(v, 10, 64)
	if err != nil {
		return def
	}
	return n
}

func envParseUint(key string, def uint64) uint64 {
	v, ok := ReadEnvOrFile(key)
	if !ok {
		return def
	}
	n, err := strconv.ParseUint(v, 10, 64)
	if err != nil {
		return def
	}
	return n
}

func envParseFloat(key string, def float64) float64 {
	v, ok := ReadEnvOrFile(key)
	if !ok {
		return def
	}
	f, err := strconv.ParseFloat(v, 64)
	if err != nil {
		return def
	}
	return f
}

// truthy reports whether raw is one of 1/true/yes/on (case-insensitive).
func truthy(raw string) bool {
	switch strings.ToLower(strings.TrimSpace(raw)) {
	case "1", "true", "yes", "on":
		return true
	}
	return false
}

// EnvFlag is true when KEY (or KEY_FILE) is 1/true/yes/on.
func EnvFlag(key string) bool {
	v, ok := ReadEnvOrFile(key)
	return ok && truthy(v)
}

// AppEnv is the deployment environment from APP_ENV, lowercased and
// trimmed. Unset means "production" (fail closed).
func AppEnv() string {
	v, ok := os.LookupEnv("APP_ENV")
	if !ok {
		return "production"
	}
	return strings.ToLower(strings.TrimSpace(v))
}

// IsProduction reports AppEnv() == "production".
func IsProduction() bool { return AppEnv() == "production" }

// IsDevEnv reports development-like environments ("development", "dev",
// "test"): dev CSP, no HSTS, test endpoints. Anything else (staging,
// preview, typos) is treated like production.
func IsDevEnv(env string) bool {
	switch env {
	case "development", "dev", "test":
		return true
	}
	return false
}

// OpenAPIDocsEnabled: OpenAPI UI/spec routes respond only when APP_ENV is
// development or dev.
func OpenAPIDocsEnabled() bool {
	switch AppEnv() {
	case "development", "dev":
		return true
	}
	return false
}

// SecureCookies is false only in development/dev, where cookies drop
// Secure and the __Host- prefix (local HTTP could never send them back).
// Test runs use production-style cookies unless they set APP_ENV=development.
func SecureCookies() bool {
	switch AppEnv() {
	case "development", "dev":
		return false
	}
	return true
}
