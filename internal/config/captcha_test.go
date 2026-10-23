package config

import (
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func unsetTurnstile(t *testing.T) {
	t.Helper()
	unset(t, "TURNSTILE_SITE_KEY", "TURNSTILE_SITE_KEY_FILE", "TURNSTILE_SECRET_KEY",
		"TURNSTILE_SECRET_KEY_FILE", "TURNSTILE_ENFORCE", "TURNSTILE_ENFORCE_FILE", "TURNSTILE_VERIFY_URL")
}

func TestCaptchaFromEnv(t *testing.T) {
	unsetTurnstile(t)
	t.Setenv("APP_ENV", "test")
	c := CaptchaFromEnv()
	if c.Configured() || c.Enforced || c.WidgetSiteKey() != "" || c.VerifyURL != TurnstileVerifyURL {
		t.Fatalf("unset: %v", c)
	}
	secretFile := filepath.Join(t.TempDir(), "turnstile")
	if err := os.WriteFile(secretFile, []byte("file-secret\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	t.Setenv("TURNSTILE_SITE_KEY", " site ")
	t.Setenv("TURNSTILE_SECRET_KEY_FILE", secretFile)
	t.Setenv("TURNSTILE_ENFORCE", "true")
	t.Setenv("TURNSTILE_VERIFY_URL", "http://127.0.0.1:9/siteverify")
	c = CaptchaFromEnv()
	if !c.Configured() || !c.Enforced || c.SiteKey != "site" || c.Secret != "file-secret" ||
		c.WidgetSiteKey() != "site" || c.VerifyURL != "http://127.0.0.1:9/siteverify" {
		t.Fatalf("configured: %v", c)
	}
	if strings.Contains(fmt.Sprintf("%v %#v %s", c, c, c), "file-secret") {
		t.Fatal("secret leaked through fmt")
	}
	// The verify URL override is dev/test only.
	t.Setenv("APP_ENV", "production")
	if c := CaptchaFromEnv(); c.VerifyURL != TurnstileVerifyURL {
		t.Fatalf("production must ignore TURNSTILE_VERIFY_URL: %s", c.VerifyURL)
	}
}

func TestLoadRequiresTurnstileKeysWhenEnforcedInProduction(t *testing.T) {
	prodBase(t)
	unsetTurnstile(t)
	t.Setenv("TURNSTILE_ENFORCE", "1")
	if _, err := loadCore(); !errors.Is(err, ErrTurnstileEnforcedWithoutKeys) {
		t.Fatalf("no keys: %v", err)
	}
	t.Setenv("TURNSTILE_SECRET_KEY", "secret")
	if _, err := loadCore(); !errors.Is(err, ErrTurnstileEnforcedWithoutKeys) {
		t.Fatalf("no site key: %v", err)
	}
	t.Setenv("TURNSTILE_SITE_KEY", "site")
	cfg, err := loadCore()
	if err != nil || !cfg.Captcha.Configured() || !cfg.Captcha.Enforced {
		t.Fatalf("configured: %v %v", cfg, err)
	}
	// Not enforced: missing keys are fine (no-op).
	unsetTurnstile(t)
	if _, err := loadCore(); err != nil {
		t.Fatal(err)
	}
	// Outside production, enforcement without keys only warns.
	t.Setenv("APP_ENV", "development")
	t.Setenv("TURNSTILE_ENFORCE", "1")
	if _, err := loadCore(); err != nil {
		t.Fatal(err)
	}
}
