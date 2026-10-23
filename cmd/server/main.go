// Command server runs the Lindaflor web application.
package main

import (
	"context"
	"errors"
	"flag"
	"fmt"
	"log/slog"
	"net"
	"net/http"
	"os"
	"os/signal"
	"strconv"
	"syscall"
	"time"

	"github.com/wellington-vell/lindaflor/internal/app"
	"github.com/wellington-vell/lindaflor/internal/config"
	"github.com/wellington-vell/lindaflor/internal/db"
	"github.com/wellington-vell/lindaflor/internal/db/sqlc"
	"github.com/wellington-vell/lindaflor/internal/jobs"
	"github.com/wellington-vell/lindaflor/internal/logging"
	"github.com/wellington-vell/lindaflor/internal/mail"
	"github.com/wellington-vell/lindaflor/internal/payments"
	"github.com/wellington-vell/lindaflor/internal/storage"
	"github.com/wellington-vell/lindaflor/internal/store"
	"github.com/wellington-vell/lindaflor/internal/valkey"
	"github.com/wellington-vell/lindaflor/internal/web"
)

const shutdownTimeout = 30 * time.Second

func main() {
	healthcheck := flag.Bool("healthcheck", false,
		"probe GET -healthcheck-path on 127.0.0.1:$PORT and exit 0 (2xx) or 1 (container HEALTHCHECK; the runtime image has no curl)")
	healthPath := flag.String("healthcheck-path", "/api/health",
		"path probed by -healthcheck (/api/health = liveness, /api/readyz = readiness)")
	flag.Parse()
	if *healthcheck {
		os.Exit(probeHealth(*healthPath))
	}
	if err := run(); err != nil {
		slog.Error("server exited", "error", err)
		os.Exit(1)
	}
}

func run() error {
	ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer stop()

	cfg, err := config.Load()
	if err != nil {
		return fmt.Errorf("config: %w", err)
	}
	logger := logging.New(cfg)
	slog.SetDefault(logger)

	pool, err := db.Open(ctx, cfg.DatabaseURL, db.OptionsFromConfig(cfg))
	if err != nil {
		return err
	}
	defer pool.Close()

	vk, err := valkey.Connect(ctx, cfg.ValkeyURL)
	if err != nil {
		return err
	}
	defer func() { _ = vk.Close() }()

	objects := storage.FromConfig(ctx, cfg.S3)

	mailer, err := mail.FromConfig(cfg.Mail, cfg.AppEnv == "production")
	if err != nil {
		return fmt.Errorf("mail: %w", err)
	}

	pay := payments.FromConfig(cfg.Payments)
	logger.Info("payment provider", "provider", string(pay.Kind()))

	deps := &app.Deps{
		Cfg: cfg, Pool: pool, Q: sqlc.New(pool), Valkey: vk,
		Mailer: mailer, Storage: objects, Payments: pay, Logger: logger,
		Orders: store.OrderFSM{Pool: pool},
	}
	store.SetLinks(mail.LinksFromConfig(cfg))
	handler, err := web.NewRouter(deps)
	if err != nil {
		return err
	}

	jobsCtx, cancelJobs := context.WithCancel(context.Background())
	jobsDone := jobs.Start(jobsCtx, deps)

	srv := &http.Server{
		Addr:              net.JoinHostPort(cfg.Host, strconv.Itoa(cfg.Port)),
		Handler:           handler,
		ReadHeaderTimeout: 10 * time.Second,
		ReadTimeout:       60 * time.Second,
		WriteTimeout:      90 * time.Second,
		IdleTimeout:       120 * time.Second,
		MaxHeaderBytes:    1 << 20,
		ErrorLog:          slog.NewLogLogger(logger.Handler(), slog.LevelWarn),
	}
	errCh := make(chan error, 1)
	go func() {
		logger.Info("listening", "addr", "http://"+srv.Addr, "env", cfg.AppEnv)
		if err := srv.ListenAndServe(); err != nil && !errors.Is(err, http.ErrServerClosed) {
			errCh <- err
		}
		close(errCh)
	}()

	select {
	case err := <-errCh:
		cancelJobs()
		return err
	case <-ctx.Done():
	}
	logger.Info("shutting down", "timeout", shutdownTimeout)
	shutdownCtx, cancel := context.WithTimeout(context.Background(), shutdownTimeout)
	defer cancel()
	err = srv.Shutdown(shutdownCtx)
	cancelJobs()
	jobsDone.Wait()
	return err
}

// probeHealth is the container probe: a GET of path (/api/health or
// /api/readyz) on the loopback interface, using PORT like the server does.
func probeHealth(path string) int {
	if path != "/api/health" && path != "/api/readyz" {
		fmt.Fprintln(os.Stderr, "healthcheck: -healthcheck-path must be /api/health or /api/readyz")
		return 1
	}
	port := 4200
	if raw := os.Getenv("PORT"); raw != "" {
		p, err := strconv.ParseUint(raw, 10, 16)
		if err != nil {
			fmt.Fprintln(os.Stderr, "healthcheck: invalid PORT")
			return 1
		}
		port = int(p)
	}
	url := "http://" + net.JoinHostPort("127.0.0.1", strconv.Itoa(port)) + path
	ctx, cancel := context.WithTimeout(context.Background(), 3*time.Second)
	defer cancel()
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, url, nil) //nolint:gosec // fixed loopback URL
	if err != nil {
		fmt.Fprintln(os.Stderr, "healthcheck:", err)
		return 1
	}
	res, err := http.DefaultClient.Do(req) //nolint:gosec // fixed loopback URL
	if err != nil {
		fmt.Fprintln(os.Stderr, "healthcheck:", err)
		return 1
	}
	_ = res.Body.Close()
	if res.StatusCode < 200 || res.StatusCode > 299 {
		fmt.Fprintln(os.Stderr, "healthcheck: status", res.StatusCode)
		return 1
	}
	return 0
}
