// Command server runs the Lindaflor web application.
package main

import (
	"context"
	"errors"
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
	"github.com/wellington-vell/lindaflor/internal/valkey"
	"github.com/wellington-vell/lindaflor/internal/web"
)

const shutdownTimeout = 30 * time.Second

func main() {
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

	store := storage.FromConfig(ctx, cfg.S3)

	mailer, err := mail.FromConfig(cfg.Mail, cfg.AppEnv == "production")
	if err != nil {
		return fmt.Errorf("mail: %w", err)
	}

	pay := payments.FromConfig(cfg.Payments)
	logger.Info("payment provider", "provider", string(pay.Kind()))

	deps := &app.Deps{
		Cfg: cfg, Pool: pool, Q: sqlc.New(pool), Valkey: vk,
		Mailer: mailer, Storage: store, Payments: pay, Logger: logger,
	}
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
