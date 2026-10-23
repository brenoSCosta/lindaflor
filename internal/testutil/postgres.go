// Package testutil provides integration-test plumbing: one Postgres server
// per test binary, a fresh migrated database per test (cloned from a
// template, ~10ms), miniredis-backed Valkey, a full-router test server and
// a cookie-jar HTTP client with helpers mirroring crates/test-support.
//
// Postgres: binaries on PATH (`initdb` + `postgres`, which the nix dev
// shell provides) are preferred; when missing or broken,
// fergusstrange/embedded-postgres downloads a server
// (LINDAFLOR_TEST_PG=local|embedded forces one). The server dies with the test process (Pdeathsig);
// call testutil.Main from TestMain to also remove its data directory
// eagerly (stale directories are swept on the next run).
package testutil

import (
	"context"
	"crypto/rand"
	"encoding/hex"
	"errors"
	"fmt"
	"net"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strconv"
	"strings"
	"sync"
	"syscall"
	"testing"
	"time"

	embeddedpostgres "github.com/fergusstrange/embedded-postgres"
	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/wellington-vell/lindaflor/internal/db"
)

const (
	pgUser       = "postgres"
	templateName = "lindaflor_template"
	dirPrefix    = "lindaflor-testpg-"
)

type server struct {
	port    int
	auth    string // "user" or "user:password"
	dataDir string
	stop    func()
}

var (
	srvOnce sync.Once
	srv     *server
	srvErr  error
	tmplMu  sync.Mutex // serializes CREATE DATABASE ... TEMPLATE
)

// url is the DSN of dbname on the shared server.
func (s *server) url(dbname string) string {
	auth := s.auth
	if auth == "" {
		auth = pgUser
	}
	return fmt.Sprintf("postgres://%s@127.0.0.1:%d/%s?sslmode=disable", auth, s.port, dbname)
}

func freePort() (int, error) {
	l, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		return 0, err
	}
	defer func() { _ = l.Close() }()
	return l.Addr().(*net.TCPAddr).Port, nil
}

// ensureServer starts the shared server once. LINDAFLOR_TEST_PG=local or
// =embedded forces a mode; by default PATH binaries are tried first and
// embedded-postgres is the fallback when they are missing or fail.
func ensureServer() (*server, error) {
	srvOnce.Do(func() {
		sweepStaleDirs()
		mode := os.Getenv("LINDAFLOR_TEST_PG")
		var localErr error
		if mode != "embedded" && hasLocalBinaries() {
			srv, localErr = startLocal()
			if localErr == nil || mode == "local" {
				srvErr = localErr
				if srvErr == nil {
					srvErr = prepareTemplate(srv)
				}
				return
			}
		}
		srv, srvErr = startEmbedded()
		if srvErr != nil && localErr != nil {
			srvErr = errors.Join(localErr, srvErr)
		}
		if srvErr == nil {
			srvErr = prepareTemplate(srv)
		}
	})
	return srv, srvErr
}

func hasLocalBinaries() bool {
	_, e1 := exec.LookPath("initdb")
	_, e2 := exec.LookPath("postgres")
	return e1 == nil && e2 == nil
}

// startLocal runs initdb + postgres from PATH. The postmaster is spawned
// from a goroutine that keeps its OS thread for the life of the process,
// so Pdeathsig (SIGQUIT = immediate shutdown) fires only when the test
// binary exits.
func startLocal() (*server, error) {
	dir, err := os.MkdirTemp("", dirPrefix+strconv.Itoa(os.Getpid())+"-")
	if err != nil {
		return nil, err
	}
	dataDir := filepath.Join(dir, "data")
	out, err := exec.Command("initdb", "-D", dataDir, "-U", pgUser, "--auth=trust", //nolint:gosec // test-only fixed binary
		"--no-sync", "-E", "UTF8", "--locale=C").CombinedOutput()
	if err != nil {
		return nil, fmt.Errorf("initdb: %w: %s", err, out)
	}
	port, err := freePort()
	if err != nil {
		return nil, err
	}
	type started struct {
		cmd *exec.Cmd
		err error
	}
	ch := make(chan started, 1)
	go func() {
		runtime.LockOSThread()                                                   // never unlocked: Pdeathsig is tied to this thread
		cmd := exec.Command("postgres", "-D", dataDir, "-p", strconv.Itoa(port), //nolint:gosec // test-only
			"-c", "listen_addresses=127.0.0.1",
			"-c", "unix_socket_directories=",
			"-c", "fsync=off", "-c", "synchronous_commit=off", "-c", "full_page_writes=off",
			"-c", "max_connections=300", "-c", "shared_buffers=64MB")
		cmd.SysProcAttr = &syscall.SysProcAttr{Pdeathsig: syscall.SIGQUIT, Setpgid: true}
		logf, _ := os.Create(filepath.Join(dir, "postgres.log")) //nolint:gosec // test-only temp dir
		cmd.Stdout, cmd.Stderr = logf, logf
		err := cmd.Start()
		ch <- started{cmd, err}
		if err == nil {
			_ = cmd.Wait()
		}
		select {} // keep the thread alive
	}()
	st := <-ch
	if st.err != nil {
		return nil, fmt.Errorf("start postgres: %w", st.err)
	}
	s := &server{port: port, dataDir: dir, stop: func() {
		_ = st.cmd.Process.Signal(syscall.SIGQUIT)
		time.Sleep(200 * time.Millisecond)
		_ = os.RemoveAll(dir)
	}}
	if err := waitReady(s); err != nil {
		logs, _ := os.ReadFile(filepath.Join(dir, "postgres.log")) //nolint:gosec // test-only
		return nil, fmt.Errorf("%w\n%s", err, logs)
	}
	return s, nil
}

func startEmbedded() (*server, error) {
	dir, err := os.MkdirTemp("", dirPrefix+strconv.Itoa(os.Getpid())+"-")
	if err != nil {
		return nil, err
	}
	port, err := freePort()
	if err != nil {
		return nil, err
	}
	pg := embeddedpostgres.NewDatabase(embeddedpostgres.DefaultConfig().
		Version(embeddedpostgres.V17).
		Port(uint32(port)). //nolint:gosec // port < 65536
		Username(pgUser).Password("postgres").Database("postgres").
		RuntimePath(filepath.Join(dir, "runtime")).
		DataPath(filepath.Join(dir, "data")).
		StartTimeout(60 * time.Second).
		Logger(nil))
	if err := pg.Start(); err != nil {
		return nil, fmt.Errorf("embedded postgres: %w", err)
	}
	s := &server{port: port, auth: pgUser + ":postgres", dataDir: dir, stop: func() {
		_ = pg.Stop()
		_ = os.RemoveAll(dir)
	}}
	return s, waitReady(s)
}

func waitReady(s *server) error {
	deadline := time.Now().Add(30 * time.Second)
	for {
		ctx, cancel := context.WithTimeout(context.Background(), time.Second)
		conn, err := pgx.Connect(ctx, s.url("postgres"))
		cancel()
		if err == nil {
			_ = conn.Close(context.Background())
			return nil
		}
		if time.Now().After(deadline) {
			return fmt.Errorf("postgres not ready: %w", err)
		}
		time.Sleep(50 * time.Millisecond)
	}
}

func prepareTemplate(s *server) error {
	ctx := context.Background()
	if err := exec1(ctx, s.url("postgres"), "CREATE DATABASE "+templateName); err != nil {
		return err
	}
	pool, err := pgxpool.New(ctx, s.url(templateName))
	if err != nil {
		return err
	}
	err = db.Migrate(ctx, pool)
	pool.Close()
	if err != nil {
		return fmt.Errorf("migrate template: %w", err)
	}
	return exec1(ctx, s.url("postgres"), "ALTER DATABASE "+templateName+" WITH IS_TEMPLATE true")
}

func exec1(ctx context.Context, dsn, sql string) error {
	conn, err := pgx.Connect(ctx, dsn)
	if err != nil {
		return err
	}
	defer func() { _ = conn.Close(ctx) }()
	_, err = conn.Exec(ctx, sql)
	return err
}

// sweepStaleDirs removes data directories left by test processes that no
// longer exist.
func sweepStaleDirs() {
	matches, _ := filepath.Glob(filepath.Join(os.TempDir(), dirPrefix+"*"))
	for _, m := range matches {
		rest := strings.TrimPrefix(filepath.Base(m), dirPrefix)
		pidStr, _, _ := strings.Cut(rest, "-")
		pid, err := strconv.Atoi(pidStr)
		if err != nil || pid == os.Getpid() {
			continue
		}
		if syscall.Kill(pid, 0) == nil || !errors.Is(syscall.Kill(pid, 0), syscall.ESRCH) {
			continue // still running (or not ours to judge)
		}
		_ = os.RemoveAll(m)
	}
}

// Main wraps testing.M: runs the tests, then stops the shared Postgres and
// removes its data directory.
//
//	func TestMain(m *testing.M) { testutil.Main(m) }
func Main(m *testing.M) {
	code := m.Run()
	if srv != nil && srv.stop != nil {
		srv.stop()
	}
	os.Exit(code)
}

func randomName() string {
	b := make([]byte, 8)
	_, _ = rand.Read(b)
	return "t_" + hex.EncodeToString(b)
}

// NewDatabase creates a fresh database cloned from the migrated template
// and returns its URL. It is dropped when the test ends.
func NewDatabase(t testing.TB) string {
	t.Helper()
	s, err := ensureServer()
	if err != nil {
		t.Fatalf("testutil: postgres: %v", err)
	}
	name := randomName()
	ctx := context.Background()
	tmplMu.Lock()
	err = exec1(ctx, s.url("postgres"), "CREATE DATABASE "+name+" TEMPLATE "+templateName)
	tmplMu.Unlock()
	if err != nil {
		t.Fatalf("testutil: create database: %v", err)
	}
	t.Cleanup(func() {
		_ = exec1(context.Background(), s.url("postgres"), "DROP DATABASE IF EXISTS "+name+" WITH (FORCE)")
	})
	return s.url(name)
}

// NewEmptyDatabase creates a database with no schema (for migration tests).
func NewEmptyDatabase(t testing.TB) string {
	t.Helper()
	s, err := ensureServer()
	if err != nil {
		t.Fatalf("testutil: postgres: %v", err)
	}
	name := randomName()
	if err := exec1(context.Background(), s.url("postgres"), "CREATE DATABASE "+name); err != nil {
		t.Fatalf("testutil: create database: %v", err)
	}
	t.Cleanup(func() {
		_ = exec1(context.Background(), s.url("postgres"), "DROP DATABASE IF EXISTS "+name+" WITH (FORCE)")
	})
	return s.url(name)
}

// NewPool returns a pool on a fresh migrated database (closed at cleanup).
func NewPool(t testing.TB) *pgxpool.Pool {
	t.Helper()
	cfg, err := pgxpool.ParseConfig(NewDatabase(t))
	if err != nil {
		t.Fatal(err)
	}
	cfg.MaxConns = 5
	pool, err := pgxpool.NewWithConfig(context.Background(), cfg)
	if err != nil {
		t.Fatalf("testutil: pool: %v", err)
	}
	t.Cleanup(pool.Close)
	return pool
}
