package store

import (
	"bytes"
	"context"
	"errors"
	"fmt"
	"log/slog"
	"strings"

	"github.com/google/uuid"

	"github.com/wellington-vell/lindaflor/internal/storage"
)

// Image upload pipeline for products, sets and banners (src/app/utils.rs):
// magic-byte validation, storage keys vs external URLs, presigned reads.
// Uploaded bytes always land in the object store, never in the repo tree.

// Image limits and messages.
const (
	MaxImageBytes          = 2 * 1024 * 1024
	MsgImageType           = "Apenas imagens JPG, PNG ou WebP são permitidas"
	MsgImageSize           = "A imagem deve ter no máximo 2MB"
	MsgStorageUnavailable  = "Serviço de arquivos temporariamente indisponível. Tente novamente mais tarde."
	MaxImagesPerProduct    = 8
	MaxImagesPerSet        = 8
	MsgProductImageCount   = "O produto pode ter no máximo 8 imagens"
	MsgSetImageCount       = "O conjunto pode ter no máximo 8 imagens"
)

// Upload is one uploaded file of a multipart form.
type Upload struct {
	ContentType string
	Bytes       []byte
}

// ImageKind is a validated image type.
type ImageKind struct {
	Extension   string
	ContentType string
}

var pngMagic = []byte{0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A}

// ClassifyImage checks the declared type against the magic bytes.
func ClassifyImage(contentType string, b []byte) (ImageKind, bool) {
	declared := strings.TrimSpace(strings.SplitN(contentType, ";", 2)[0])
	switch {
	case strings.EqualFold(declared, "image/jpeg") && len(b) >= 3 && b[0] == 0xFF && b[1] == 0xD8 && b[2] == 0xFF:
		return ImageKind{"jpg", "image/jpeg"}, true
	case strings.EqualFold(declared, "image/png") && bytes.HasPrefix(b, pngMagic):
		return ImageKind{"png", "image/png"}, true
	case strings.EqualFold(declared, "image/webp") && len(b) >= 12 && bytes.HasPrefix(b, []byte("RIFF")) && string(b[8:12]) == "WEBP":
		return ImageKind{"webp", "image/webp"}, true
	}
	return ImageKind{}, false
}

// ValidateImage classifies and size-checks one upload (*UserError).
func ValidateImage(b []byte, contentType string) (ImageKind, error) {
	kind, ok := ClassifyImage(contentType, b)
	if !ok {
		return ImageKind{}, userErr(MsgImageType)
	}
	if len(b) > MaxImageBytes {
		return ImageKind{}, userErr(MsgImageSize)
	}
	return kind, nil
}

// IsStorageKey: an object-store key rather than an external URL.
func IsStorageKey(v string) bool { return v != "" && !strings.Contains(v, "://") }

// ResolveStorageURL turns a stored key-or-URL into a browser-loadable URL:
// external URLs pass through, keys are presigned; "" (and failed presigns)
// stay "".
func ResolveStorageURL(ctx context.Context, st storage.Store, value *string) string {
	if value == nil || *value == "" {
		return ""
	}
	if strings.Contains(*value, "://") {
		return *value
	}
	if st == nil {
		return ""
	}
	u, err := st.PresignGet(ctx, *value)
	if err != nil {
		return ""
	}
	return u
}

func resolvePtr(ctx context.Context, st storage.Store, v *string) *string {
	if u := ResolveStorageURL(ctx, st, v); u != "" {
		return &u
	}
	return nil
}

// ResolveSummaryImages resolves the cover/secondary images of cards in
// place and returns the slice.
func ResolveSummaryImages(ctx context.Context, st storage.Store, products []ProductSummary) []ProductSummary {
	for i := range products {
		products[i].ImageURL = resolvePtr(ctx, st, products[i].ImageURL)
		products[i].SecondaryImageURL = resolvePtr(ctx, st, products[i].SecondaryImageURL)
	}
	return products
}

// ResolveDetailImages resolves a product's gallery, dropping unresolvable
// images.
func ResolveDetailImages(ctx context.Context, st storage.Store, p *ProductDetail) {
	kept := p.Images[:0]
	for _, img := range p.Images {
		if u := ResolveStorageURL(ctx, st, &img.URL); u != "" {
			img.URL = u
			kept = append(kept, img)
		}
	}
	p.Images = kept
}

// ResolveCartImages resolves cart line images in place.
func ResolveCartImages(ctx context.Context, st storage.Store, items []CartItem) {
	for i := range items {
		items[i].ImageURL = resolvePtr(ctx, st, items[i].ImageURL)
	}
}

func storageFailure(err error) error {
	if errors.Is(err, storage.ErrUnavailable) {
		return userErr(MsgStorageUnavailable)
	}
	return fmt.Errorf("storage put: %w", err)
}

var (
	sqlInsertProductImage = sqlText(`INSERT INTO product_images (id, product_id, url, alt, sort_order, created_at)
 VALUES ($1, $2, $3, NULL, $4, now())`)
	sqlProductImageURL    = sqlText(`SELECT url FROM product_images WHERE id = $1`)
	sqlDeleteProductImage = sqlText(`DELETE FROM product_images WHERE id = $1`)
	sqlInsertSetImage     = sqlText(`INSERT INTO set_images (id, set_id, url, alt, sort_order, created_at)
 VALUES ($1, $2, $3, NULL, $4, now())`)
	sqlSetImageURL    = sqlText(`SELECT url FROM set_images WHERE id = $1`)
	sqlDeleteSetImage = sqlText(`DELETE FROM set_images WHERE id = $1`)
)

// saveImage validates, stores under prefix/{owner}/<uuid>.<ext> and inserts
// the row (deleting the object when the insert fails).
func saveImage(ctx context.Context, q DBTX, st storage.Store, prefix, insertSQL string, ownerID uuid.UUID, up Upload, sortOrder int32) (string, error) {
	kind, err := ValidateImage(up.Bytes, up.ContentType)
	if err != nil {
		return "", err
	}
	if storage.IsUnavailable(st) {
		return "", userErr(MsgStorageUnavailable)
	}
	key := fmt.Sprintf("%s/%s/%s.%s", prefix, ownerID, uuid.Must(uuid.NewV7()), kind.Extension)
	if err := st.Put(ctx, key, up.Bytes, kind.ContentType); err != nil {
		return "", storageFailure(err)
	}
	if _, err := q.Exec(ctx, insertSQL, uuid.Must(uuid.NewV7()), ownerID, key, sortOrder); err != nil {
		if derr := st.Delete(ctx, key); derr != nil {
			slog.WarnContext(ctx, "orphaned image object", "key", key, "error", derr)
		}
		return "", err
	}
	return key, nil
}

// SaveProductImage stores a product image (products/{id}/…).
func SaveProductImage(ctx context.Context, q DBTX, st storage.Store, productID uuid.UUID, up Upload, sortOrder int32) (string, error) {
	return saveImage(ctx, q, st, "products", sqlInsertProductImage, productID, up, sortOrder)
}

// SaveSetImage stores a set image (sets/{id}/…).
func SaveSetImage(ctx context.Context, q DBTX, st storage.Store, setID uuid.UUID, up Upload, sortOrder int32) (string, error) {
	return saveImage(ctx, q, st, "sets", sqlInsertSetImage, setID, up, sortOrder)
}

func deleteImage(ctx context.Context, q DBTX, st storage.Store, selectSQL, deleteSQL, keyPrefix string, imageID uuid.UUID) error {
	var url string
	err := q.QueryRow(ctx, selectSQL, imageID).Scan(&url)
	if found, err := optional(err); err != nil || !found {
		return err
	}
	if _, err := q.Exec(ctx, deleteSQL, imageID); err != nil {
		return err
	}
	if IsStorageKey(url) && strings.HasPrefix(url, keyPrefix) && !storage.IsUnavailable(st) {
		_ = st.Delete(ctx, url)
	}
	return nil
}

// DeleteProductImage removes the row and its stored object (when a key).
func DeleteProductImage(ctx context.Context, q DBTX, st storage.Store, imageID uuid.UUID) error {
	return deleteImage(ctx, q, st, sqlProductImageURL, sqlDeleteProductImage, "", imageID)
}

// DeleteSetImage removes the row and its stored object (sets/ keys only).
func DeleteSetImage(ctx context.Context, q DBTX, st storage.Store, imageID uuid.UUID) error {
	return deleteImage(ctx, q, st, sqlSetImageURL, sqlDeleteSetImage, "sets/", imageID)
}
