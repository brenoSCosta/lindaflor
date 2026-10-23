package main

import (
	"bytes"
	"context"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/wellington-vell/lindaflor/internal/testutil"
)

func TestMain(m *testing.M) { testutil.Main(m) }

func TestParseArgs(t *testing.T) {
	a, err := parseArgs([]string{"--email", "a@b.com", "--name=Dona", "--no-password"})
	if err != nil || a.email != "a@b.com" || a.name != "Dona" || !a.noPassword {
		t.Fatalf("%+v %v", a, err)
	}
	a, err = parseArgs([]string{"--email=x@y.z"})
	if err != nil || a.name != "Admin" || a.noPassword {
		t.Fatalf("defaults: %+v %v", a, err)
	}
	for _, bad := range [][]string{{}, {"--name", "x"}, {"--bogus"}, {"-h"}} {
		if _, err := parseArgs(bad); err == nil || !strings.Contains(err.Error(), "usage:") {
			t.Errorf("%v: %v", bad, err)
		}
	}
}

func TestRunCreatesAndPromotes(t *testing.T) {
	pool := testutil.NewPool(t)
	t.Setenv("DATABASE_URL", pool.Config().ConnString())
	dir := t.TempDir()
	file := filepath.Join(dir, "pw")
	if err := os.WriteFile(file, []byte("correct horse battery staple 42!\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	t.Setenv("ADMIN_PASSWORD_FILE", file)
	email := testutil.UniqueEmail("cli")
	var out bytes.Buffer
	if err := run(context.Background(), []string{"--email", email}, strings.NewReader(""), &out, &out); err != nil {
		t.Fatal(err)
	}
	if !strings.HasPrefix(out.String(), "created admin ") {
		t.Fatal(out.String())
	}
	out.Reset()
	t.Setenv("ADMIN_PASSWORD_FILE", "")
	if err := run(context.Background(), []string{"--email", email, "--no-password"}, strings.NewReader(""), &out, &out); err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(out.String(), "to admin") {
		t.Fatal(out.String())
	}
	// Password from stdin; empty is refused.
	if err := run(context.Background(), []string{"--email", email}, strings.NewReader("\n"), &out, &out); err == nil || !strings.Contains(err.Error(), "empty password") {
		t.Fatalf("empty: %v", err)
	}
	if err := run(context.Background(), []string{"--email", email}, strings.NewReader("weak\n"), &out, &out); err == nil {
		t.Fatal("weak stdin password must be refused")
	}
	var role string
	if err := pool.QueryRow(context.Background(), `SELECT role FROM users WHERE email = $1`, email).Scan(&role); err != nil || role != "admin" {
		t.Fatal(role, err)
	}
}
