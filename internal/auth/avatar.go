package auth

import (
	"bytes"
	"context"
	"errors"
	"fmt"
	"strings"

	"github.com/google/uuid"
	"github.com/jackc/pgx/v5"

	"github.com/wellington-vell/lindaflor/internal/db/sqlc"
	"github.com/wellington-vell/lindaflor/internal/storage"
)

// Upload limits and pt-BR messages (src/app/utils.rs).
const (
	MaxImageBytes  = 2 * 1024 * 1024
	MsgImageType   = "Apenas imagens JPG, PNG ou WebP são permitidas"
	MsgImageSize   = "A imagem deve ter no máximo 2MB"
	MsgUnavailable = "Serviço de arquivos temporariamente indisponível. Tente novamente mais tarde."
	msgUpdate      = "Falha ao atualizar avatar"
)

// UserMessageError carries a pt-BR message meant for the UI verbatim.
type UserMessageError struct{ Msg string }

func (e *UserMessageError) Error() string { return e.Msg }

func userMsg(m string) error { return &UserMessageError{Msg: m} }

// ImageKind is a validated upload type.
type ImageKind struct{ Extension, ContentType string }

var pngMagic = []byte{0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A}

func isJPEG(b []byte) bool { return len(b) >= 3 && b[0] == 0xFF && b[1] == 0xD8 && b[2] == 0xFF }
func isPNG(b []byte) bool  { return bytes.HasPrefix(b, pngMagic) }
func isWebP(b []byte) bool {
	return len(b) >= 12 && string(b[0:4]) == "RIFF" && string(b[8:12]) == "WEBP"
}

// ClassifyImage matches the declared content type against the magic bytes.
func ClassifyImage(contentType string, b []byte) (ImageKind, bool) {
	declared, _, _ := strings.Cut(contentType, ";")
	declared = strings.TrimSpace(declared)
	switch {
	case strings.EqualFold(declared, "image/jpeg") && isJPEG(b):
		return ImageKind{"jpg", "image/jpeg"}, true
	case strings.EqualFold(declared, "image/png") && isPNG(b):
		return ImageKind{"png", "image/png"}, true
	case strings.EqualFold(declared, "image/webp") && isWebP(b):
		return ImageKind{"webp", "image/webp"}, true
	}
	return ImageKind{}, false
}

// ValidateImage: declared type must match magic bytes, payload ≤ 2 MiB.
func ValidateImage(b []byte, contentType string) (ImageKind, error) {
	kind, ok := ClassifyImage(contentType, b)
	if !ok {
		return ImageKind{}, userMsg(MsgImageType)
	}
	if len(b) > MaxImageBytes {
		return ImageKind{}, userMsg(MsgImageSize)
	}
	return kind, nil
}

// IsOwnAvatarKey reports a key this server issued for userID
// (avatars/<user_id>/<file>, no traversal). Only these keys are presigned
// as the user's avatar or deleted on replace/remove.
func IsOwnAvatarKey(userID uuid.UUID, value string) bool {
	file, ok := strings.CutPrefix(value, "avatars/"+userID.String()+"/")
	return ok && file != "" && !strings.HasPrefix(file, ".") && !strings.ContainsAny(file, "/\\?#")
}

func isGoogleAvatarURL(v string) bool {
	rest, ok := strings.CutPrefix(v, "https://")
	if !ok {
		return false
	}
	host := rest
	if i := strings.IndexAny(rest, "/?#"); i >= 0 {
		host = rest[:i]
	}
	return host == "googleusercontent.com" || strings.HasSuffix(host, ".googleusercontent.com")
}

// ResolveAvatarURL is the browser-loadable avatar of userID: own keys are
// presigned, Google profile pictures pass through, anything else is ""
// (initials fallback).
func ResolveAvatarURL(ctx context.Context, store storage.Store, userID uuid.UUID, value *string) string {
	if value == nil || *value == "" {
		return ""
	}
	if IsOwnAvatarKey(userID, *value) {
		if store == nil {
			return ""
		}
		u, err := store.PresignGet(ctx, *value)
		if err != nil {
			return ""
		}
		return u
	}
	if isGoogleAvatarURL(*value) {
		return *value
	}
	return ""
}

// ResolveStorageURL turns a stored key-or-URL into a URL: external URLs
// pass through, keys are presigned ("" on failure).
func ResolveStorageURL(ctx context.Context, store storage.Store, value string) string {
	if value == "" {
		return ""
	}
	if strings.Contains(value, "://") {
		return value
	}
	if store == nil {
		return ""
	}
	u, err := store.PresignGet(ctx, value)
	if err != nil {
		return ""
	}
	return u
}

func storageFailure(err error) error {
	if errors.Is(err, storage.ErrUnavailable) {
		return userMsg(MsgUnavailable)
	}
	return fmt.Errorf("storage: %w", err)
}

// UpdateAvatar validates and stores an avatar under avatars/<user>/<v7>.<ext>,
// saves the key and deletes the previous own object.
func (s *Service) UpdateAvatar(ctx context.Context, userID uuid.UUID, b []byte, contentType string) (string, error) {
	return UpdateAvatar(ctx, s.Q, s.Store, userID, b, contentType)
}

// RemoveAvatar clears users.image (own keys are deleted from storage).
func (s *Service) RemoveAvatar(ctx context.Context, userID uuid.UUID) error {
	return RemoveAvatar(ctx, s.Q, s.Store, userID)
}

// UpdateAvatar is the dependency-explicit form of Service.UpdateAvatar.
func UpdateAvatar(ctx context.Context, q *sqlc.Queries, store storage.Store, userID uuid.UUID, b []byte, contentType string) (string, error) {
	kind, err := ValidateImage(b, contentType)
	if err != nil {
		return "", err
	}
	if storage.IsUnavailable(store) {
		return "", userMsg(MsgUnavailable)
	}
	previous, err := q.AuthUserImage(ctx, userID)
	if err != nil && !errors.Is(err, pgx.ErrNoRows) {
		return "", err
	}
	key := fmt.Sprintf("avatars/%s/%s.%s", userID, newID(), kind.Extension)
	if err := store.Put(ctx, key, b, kind.ContentType); err != nil {
		return "", storageFailure(err)
	}
	n, err := q.AuthSetUserImage(ctx, sqlc.AuthSetUserImageParams{ID: userID, Image: &key})
	if err != nil || n == 0 {
		_ = store.Delete(ctx, key)
		if err != nil {
			return "", err
		}
		return "", userMsg(msgUpdate)
	}
	if previous != nil && IsOwnAvatarKey(userID, *previous) && *previous != key {
		_ = store.Delete(ctx, *previous)
	}
	return key, nil
}

// RemoveAvatar is the dependency-explicit form of Service.RemoveAvatar.
func RemoveAvatar(ctx context.Context, q *sqlc.Queries, store storage.Store, userID uuid.UUID) error {
	current, err := q.AuthUserImage(ctx, userID)
	if err != nil && !errors.Is(err, pgx.ErrNoRows) {
		return err
	}
	if current != nil && IsOwnAvatarKey(userID, *current) {
		if storage.IsUnavailable(store) {
			return userMsg(MsgUnavailable)
		}
		if err := store.Delete(ctx, *current); errors.Is(err, storage.ErrUnavailable) {
			return userMsg(MsgUnavailable)
		}
	}
	_, err = q.AuthSetUserImage(ctx, sqlc.AuthSetUserImageParams{ID: userID, Image: nil})
	return err
}
