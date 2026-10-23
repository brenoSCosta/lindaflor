package logging

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io/fs"
	"log/slog"
	"os"
	"path/filepath"
	"strings"
	"sync"
)

// Rotating file defaults (LOG_FILE_MAX_BYTES / LOG_FILE_MAX_FILES).
const (
	DefaultFileMaxBytes = 10 * 1024 * 1024
	DefaultFileMaxFiles = 5
)

const logExt = ".log"

var (
	warnedMu sync.Mutex
	warned   = map[string]bool{}
)

// warnOnce reports a sink failure once per scope.
func warnOnce(scope string, err error) {
	warnedMu.Lock()
	defer warnedMu.Unlock()
	if warned[scope] {
		return
	}
	warned[scope] = true
	slog.Warn(fmt.Sprintf("[logging] %s sink failed; further failures suppressed", scope), "error", err)
}

// archivePath is "<stem>.<index>.log" for "<stem>.log" (else "<path>.<index>.log").
func archivePath(path string, index uint32) string {
	stem := strings.TrimSuffix(path, logExt)
	return fmt.Sprintf("%s.%d%s", stem, index, logExt)
}

func exists(path string) bool {
	_, err := os.Stat(path)
	return err == nil
}

func shiftArchives(path string, maxFiles uint32) error {
	if oldest := archivePath(path, maxFiles); exists(oldest) {
		if err := os.Remove(oldest); err != nil {
			return err
		}
	}
	for i := maxFiles - 1; i >= 1; i-- {
		if src := archivePath(path, i); exists(src) {
			if err := os.Rename(src, archivePath(path, i+1)); err != nil {
				return err
			}
		}
	}
	return nil
}

func rotate(path string, maxFiles uint32) error {
	if !exists(path) {
		return nil
	}
	if err := shiftArchives(path, maxFiles); err != nil {
		return err
	}
	return os.Rename(path, archivePath(path, 1))
}

// FileTee appends JSON lines to path, rotating to <stem>.1.log … <stem>.N.log
// before a write would push the file past maxBytes.
type FileTee struct {
	mu       sync.Mutex
	path     string
	maxBytes uint64
	maxFiles uint32
	disabled bool
}

// NewFileTee prepares the file (creating its directory); a failure
// disables the tee (warned once) instead of failing requests.
func NewFileTee(path string, maxBytes uint64, maxFiles uint32) *FileTee {
	if maxBytes == 0 {
		maxBytes = DefaultFileMaxBytes
	}
	if maxFiles == 0 {
		maxFiles = DefaultFileMaxFiles
	}
	t := &FileTee{path: path, maxBytes: maxBytes, maxFiles: maxFiles}
	if dir := filepath.Dir(path); dir != "" {
		if err := os.MkdirAll(dir, 0o750); err != nil {
			warnOnce("file", err)
			t.disabled = true
		}
	}
	return t
}

// Write appends one line.
func (t *FileTee) Write(line []byte) error {
	t.mu.Lock()
	defer t.mu.Unlock()
	if t.disabled {
		return nil
	}
	var size uint64
	if st, err := os.Stat(t.path); err == nil {
		size = uint64(max(st.Size(), 0))
	} else if !errors.Is(err, fs.ErrNotExist) {
		return err
	}
	if size > 0 && size+uint64(len(line)) > t.maxBytes {
		if err := rotate(t.path, t.maxFiles); err != nil {
			return err
		}
	}
	f, err := os.OpenFile(t.path, os.O_CREATE|os.O_APPEND|os.O_WRONLY, 0o640) //nolint:gosec // operator-configured path
	if err != nil {
		return err
	}
	if _, err := f.Write(line); err != nil {
		_ = f.Close()
		return err
	}
	return f.Close()
}

// NewSink emits each entry through logger (level by status) and, with a
// file path, appends it as a JSON line to the rotating file.
func NewSink(logger *slog.Logger, filePath string, maxBytes uint64, maxFiles uint32) Sink {
	var tee *FileTee
	if filePath != "" {
		tee = NewFileTee(filePath, maxBytes, maxFiles)
	}
	return func(ctx context.Context, e Entry) {
		if logger != nil {
			logEntry(ctx, logger, e)
		}
		if tee == nil {
			return
		}
		b, err := json.Marshal(e)
		if err != nil {
			warnOnce("serialize", err)
			return
		}
		if err := tee.Write(append(b, '\n')); err != nil {
			warnOnce("file", err)
		}
	}
}

func logEntry(ctx context.Context, logger *slog.Logger, e Entry) {
	level := slog.LevelInfo
	if e.Status >= 500 {
		level = slog.LevelError
	}
	attrs := []slog.Attr{
		slog.String("request_id", e.RequestID.String()),
		slog.String("timestamp", e.Timestamp),
		slog.String("method", e.Method),
		slog.String("path", e.Path),
		slog.Int64("duration_ms", e.DurationMS),
		slog.String("client_ip", e.ClientIP),
		slog.String("user_agent", e.UserAgent),
	}
	if e.UserID != "" {
		attrs = append(attrs, slog.String("user_id", e.UserID), slog.String("session_id", e.SessionID))
	}
	if e.Role != "" {
		attrs = append(attrs, slog.String("role", e.Role))
	}
	attrs = append(attrs, slog.Int("status", e.Status))
	if e.Error != nil {
		attrs = append(attrs, slog.Group("error", slog.String("code", e.Error.Code), slog.String("message", e.Error.Message)))
	}
	attrs = append(attrs, slog.Bool("sampled", e.Sampled), slog.String("sample_reason", string(e.SampleReason)))
	logger.LogAttrs(ctx, level, "request", attrs...)
}
