package auth_test

import (
	"context"
	"strings"
	"testing"

	"github.com/google/uuid"
	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/wellington-vell/lindaflor/internal/auth"
	"github.com/wellington-vell/lindaflor/internal/db/sqlc"
	"github.com/wellington-vell/lindaflor/internal/storage"
	"github.com/wellington-vell/lindaflor/internal/testutil"
)

// --- tests/avatar.rs ---

func pngBytes() []byte  { return []byte{0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0} }
func jpegBytes() []byte { return []byte{0xFF, 0xD8, 0xFF, 0xD9} }
func webpBytes() []byte {
	return append(append(append([]byte("RIFF"), 0, 0, 0, 0), []byte("WEBP")...), 0, 0, 0, 0)
}

type avatarEnv struct {
	pool *pgxpool.Pool
	q    *sqlc.Queries
}

func newAvatarEnv(t *testing.T) avatarEnv {
	t.Helper()
	pool := testutil.NewPool(t)
	return avatarEnv{pool: pool, q: sqlc.New(pool)}
}

func (e avatarEnv) user(t *testing.T) uuid.UUID {
	t.Helper()
	id := uuid.New()
	if _, err := e.pool.Exec(context.Background(), `INSERT INTO users (id, name, email, email_verified) VALUES ($1, 'Avatar User', $2, true)`,
		id, testutil.UniqueEmail("avatar")); err != nil {
		t.Fatal(err)
	}
	return id
}

func (e avatarEnv) image(t *testing.T, id uuid.UUID) *string {
	t.Helper()
	img, err := e.q.AuthUserImage(context.Background(), id)
	if err != nil {
		t.Fatal(err)
	}
	return img
}

func (e avatarEnv) setImage(t *testing.T, id uuid.UUID, v string) {
	t.Helper()
	if _, err := e.pool.Exec(context.Background(), `UPDATE users SET image = $2 WHERE id = $1`, id, v); err != nil {
		t.Fatal(err)
	}
}

func presigned(t *testing.T, s storage.Store, key string) bool {
	t.Helper()
	_, err := s.PresignGet(context.Background(), key)
	return err == nil
}

func TestAcceptsPNGJPEGAndWebP(t *testing.T) {
	e := newAvatarEnv(t)
	store := storage.NewMemory()
	ctx := context.Background()
	for _, c := range []struct {
		b       []byte
		ct, ext string
	}{{pngBytes(), "image/png", "png"}, {jpegBytes(), "image/jpeg", "jpg"}, {webpBytes(), "image/webp", "webp"}} {
		id := e.user(t)
		key, err := auth.UpdateAvatar(ctx, e.q, store, id, c.b, c.ct)
		if err != nil {
			t.Fatalf("%s: %v", c.ct, err)
		}
		if !strings.HasPrefix(key, "avatars/"+id.String()+"/") || !strings.HasSuffix(key, "."+c.ext) {
			t.Fatal(key)
		}
		if img := e.image(t, id); img == nil || *img != key {
			t.Fatal("image saved")
		}
		if u := auth.ResolveStorageURL(ctx, store, key); !strings.HasPrefix(u, "memory://") {
			t.Fatal(u)
		}
		if obj, ok := store.Get(key); !ok || obj.ContentType != c.ct {
			t.Fatal("stored with the sniffed content type")
		}
	}
}

func TestRejectsGIFRandomAndOversize(t *testing.T) {
	e := newAvatarEnv(t)
	store := storage.NewMemory()
	ctx := context.Background()
	id := e.user(t)
	if _, err := auth.UpdateAvatar(ctx, e.q, store, id, []byte("GIF89a\x00\x00\x00\x00"), "image/gif"); err == nil || err.Error() != auth.MsgImageType {
		t.Fatalf("gif: %v", err)
	}
	if _, err := auth.UpdateAvatar(ctx, e.q, store, id, []byte{1, 2, 3, 4, 5}, "image/jpeg"); err == nil || err.Error() != auth.MsgImageType {
		t.Fatalf("random: %v", err)
	}
	big := append(pngBytes(), make([]byte, 2*1024*1024+1-len(pngBytes()))...)
	if _, err := auth.UpdateAvatar(ctx, e.q, store, id, big, "image/png"); err == nil || err.Error() != auth.MsgImageSize {
		t.Fatalf("oversize: %v", err)
	}
	if e.image(t, id) != nil {
		t.Fatal("image untouched")
	}
}

func TestReplaceRemovesPreviousObject(t *testing.T) {
	e := newAvatarEnv(t)
	store := storage.NewMemory()
	ctx := context.Background()
	id := e.user(t)
	first, err := auth.UpdateAvatar(ctx, e.q, store, id, pngBytes(), "image/png")
	if err != nil {
		t.Fatal(err)
	}
	second, err := auth.UpdateAvatar(ctx, e.q, store, id, jpegBytes(), "image/jpeg")
	if err != nil || first == second {
		t.Fatal(err)
	}
	if presigned(t, store, first) || !presigned(t, store, second) {
		t.Fatal("previous object removed, new one kept")
	}
	if img := e.image(t, id); img == nil || *img != second {
		t.Fatal("image is the new key")
	}
}

func TestExternalURLIsKeptAndNotStored(t *testing.T) {
	e := newAvatarEnv(t)
	store := storage.NewMemory()
	ctx := context.Background()
	id := e.user(t)
	external := "https://example.com/a.png"
	e.setImage(t, id, external)
	if u := auth.ResolveStorageURL(ctx, store, external); u != external {
		t.Fatal(u)
	}
	key, err := auth.UpdateAvatar(ctx, e.q, store, id, pngBytes(), "image/png")
	if err != nil {
		t.Fatal(err)
	}
	if img := e.image(t, id); img == nil || *img != key {
		t.Fatal("replaced by the upload")
	}
	if presigned(t, store, external) || auth.ResolveStorageURL(ctx, store, key) == "" {
		t.Fatal("external never stored; key presigns")
	}
}

func TestRemoveClearsImageAndDeletesObject(t *testing.T) {
	e := newAvatarEnv(t)
	store := storage.NewMemory()
	ctx := context.Background()
	id := e.user(t)
	key, _ := auth.UpdateAvatar(ctx, e.q, store, id, pngBytes(), "image/png")
	if err := auth.RemoveAvatar(ctx, e.q, store, id); err != nil {
		t.Fatal(err)
	}
	if e.image(t, id) != nil || presigned(t, store, key) {
		t.Fatal("cleared and deleted")
	}
}

func TestRemoveExternalURLWithUnavailableStore(t *testing.T) {
	e := newAvatarEnv(t)
	id := e.user(t)
	e.setImage(t, id, "https://example.com/a.png")
	if err := auth.RemoveAvatar(context.Background(), e.q, storage.Unavailable{}, id); err != nil {
		t.Fatal(err)
	}
	if e.image(t, id) != nil {
		t.Fatal("external url nulled")
	}
}

func TestUnavailableStoreRejectsUploadAndKeepsStorageKey(t *testing.T) {
	e := newAvatarEnv(t)
	store := storage.NewMemory()
	ctx := context.Background()
	id := e.user(t)
	if _, err := auth.UpdateAvatar(ctx, e.q, storage.Unavailable{}, id, pngBytes(), "image/png"); err == nil || err.Error() != auth.MsgUnavailable {
		t.Fatalf("upload: %v", err)
	}
	if e.image(t, id) != nil {
		t.Fatal("no image")
	}
	key, _ := auth.UpdateAvatar(ctx, e.q, store, id, pngBytes(), "image/png")
	if err := auth.RemoveAvatar(ctx, e.q, storage.Unavailable{}, id); err == nil || err.Error() != auth.MsgUnavailable {
		t.Fatalf("remove: %v", err)
	}
	if img := e.image(t, id); img == nil || *img != key || !presigned(t, store, key) {
		t.Fatal("key kept when storage is down")
	}
}

func TestForeignKeysInImageAreNeverDeletedOrPresigned(t *testing.T) {
	e := newAvatarEnv(t)
	store := storage.NewMemory()
	ctx := context.Background()
	id := e.user(t)
	product := "products/" + uuid.NewString() + "/" + uuid.NewString() + ".jpg"
	_ = store.Put(ctx, product, jpegBytes(), "image/jpeg")
	e.setImage(t, id, product)
	if u := auth.ResolveAvatarURL(ctx, store, id, &product); u != "" {
		t.Fatal("a foreign key must not be presigned as an avatar")
	}
	own, err := auth.UpdateAvatar(ctx, e.q, store, id, pngBytes(), "image/png")
	if err != nil || !auth.IsOwnAvatarKey(id, own) || !presigned(t, store, product) {
		t.Fatal("replacing must not delete the forged previous key")
	}
	other := e.user(t)
	otherKey, _ := auth.UpdateAvatar(ctx, e.q, store, other, pngBytes(), "image/png")
	e.setImage(t, id, otherKey)
	if err := auth.RemoveAvatar(ctx, e.q, store, id); err != nil {
		t.Fatal(err)
	}
	if e.image(t, id) != nil || !presigned(t, store, otherKey) {
		t.Fatal("another user's avatar is never deleted")
	}
	e.setImage(t, id, product)
	if err := auth.RemoveAvatar(ctx, e.q, store, id); err != nil {
		t.Fatal(err)
	}
	if e.image(t, id) != nil || !presigned(t, store, product) {
		t.Fatal("foreign key nulled, object kept")
	}
}

func TestOwnAvatarKeyRejectsTraversalAndOtherUsers(t *testing.T) {
	id := uuid.New()
	if !auth.IsOwnAvatarKey(id, "avatars/"+id.String()+"/a.png") {
		t.Fatal("own key")
	}
	for _, bad := range []string{
		"avatars/" + id.String() + "/../../products/x.jpg", "avatars/" + id.String() + "/",
		"avatars/" + uuid.NewString() + "/a.png", "products/p/a.jpg", "avatars/" + id.String() + "/.hidden",
	} {
		if auth.IsOwnAvatarKey(id, bad) {
			t.Errorf("%s accepted", bad)
		}
	}
}

func TestGoogleAvatarURLsPassThroughOtherURLsDoNot(t *testing.T) {
	store := storage.NewMemory()
	id := uuid.New()
	google := "https://lh3.googleusercontent.com/a/abc=s96-c"
	if u := auth.ResolveAvatarURL(context.Background(), store, id, &google); u != google {
		t.Fatal(u)
	}
	for _, other := range []string{"https://evil.example/a.png", "https://googleusercontent.com.evil.example/a.png", "http://lh3.googleusercontent.com/a.png"} {
		if u := auth.ResolveAvatarURL(context.Background(), store, id, &other); u != "" {
			t.Errorf("%s → %s", other, u)
		}
	}
}

func TestClassifyImage(t *testing.T) {
	if k, ok := auth.ClassifyImage("image/jpeg; charset=x", jpegBytes()); !ok || k.Extension != "jpg" {
		t.Fatal("jpeg")
	}
	if _, ok := auth.ClassifyImage("image/png", jpegBytes()); ok {
		t.Fatal("declared type must match the magic bytes")
	}
	if _, err := auth.ValidateImage(nil, "image/png"); err == nil || err.Error() != auth.MsgImageType {
		t.Fatal("empty payload reports the type message")
	}
}
