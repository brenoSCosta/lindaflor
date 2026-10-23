package admin

import (
	"fmt"
	"io"
	"log/slog"
	"mime/multipart"
	"net/http"
	"net/url"
	"strconv"
	"strings"

	"github.com/a-h/templ"
	"github.com/google/uuid"

	"github.com/wellington-vell/lindaflor/internal/app"
	"github.com/wellington-vell/lindaflor/internal/auth"
	"github.com/wellington-vell/lindaflor/internal/httpx"
	"github.com/wellington-vell/lindaflor/internal/httpx/toast"
	"github.com/wellington-vell/lindaflor/internal/payments"
	"github.com/wellington-vell/lindaflor/internal/seo"
	"github.com/wellington-vell/lindaflor/internal/store"
	"github.com/wellington-vell/lindaflor/internal/web/components"
	"github.com/wellington-vell/lindaflor/internal/web/format"
	"github.com/wellington-vell/lindaflor/internal/web/render"
)

// Body limits (src/app.rs): product create/update carry up to 8 images of
// 2 MiB each. Sets accept the same gallery and banners one image, so they
// get the same/an image-sized budget (the Rust app left them at the 1 MiB
// global cap, which rejected most uploads).
const (
	productBodyLimit = 20 << 20
	setBodyLimit     = 20 << 20
	bannerBodyLimit  = 3 << 20
	multipartMemory  = 32 << 20
)

const msgGenericError = "Não foi possível concluir a operação. Tente novamente."

// handler carries the dependencies of every admin route.
type handler struct {
	d    *app.Deps
	auth *auth.Service
	pay  *payments.Service
}

// actor returns the signed-in admin. The /admin group already runs
// auth.RequireAdmin; this is the defense-in-depth check the Rust shards and
// routes did themselves (403 when it fails).
func (h *handler) actor(w http.ResponseWriter, r *http.Request) (auth.Actor, bool) {
	u, ok := auth.CurrentUser(r.Context())
	if !ok || !u.IsAdmin() {
		http.Error(w, "Acesso negado.", http.StatusForbidden)
		return auth.Actor{}, false
	}
	s, _ := auth.CurrentSession(r.Context())
	return auth.Actor{User: u, Session: s}, true
}

func storeActor(a auth.Actor) store.Actor { return store.UserActor(a.User.ID, a.User.Name) }

func paymentsActor(a auth.Actor) payments.Actor {
	id := a.User.ID
	return payments.Actor{ID: &id, Name: a.User.Name}
}

// head sets the (noindex) page title.
func head(r *http.Request, title string) {
	seo.SetHead(r.Context(), seo.New(title).WithNoIndex())
}

// serverError logs err and answers 500.
func serverError(w http.ResponseWriter, r *http.Request, err error) {
	slog.ErrorContext(r.Context(), "admin handler failed", "error", err, "path", r.URL.Path)
	http.Error(w, "Erro interno.", http.StatusInternalServerError)
}

// serverLog logs an unexpected (non-user) failure.
func serverLog(r *http.Request, err error) {
	slog.ErrorContext(r.Context(), "admin action failed", "error", err, "path", r.URL.Path)
}

// parseForm parses an urlencoded form (413 when oversized, 400 when bad).
func parseForm(w http.ResponseWriter, r *http.Request) bool {
	if err := r.ParseForm(); err != nil {
		if httpx.IsBodyTooLarge(err) {
			httpx.TooLarge(w)
			return false
		}
		http.Error(w, "Requisição inválida.", http.StatusBadRequest)
		return false
	}
	return true
}

// multipartForm is a parsed multipart submission: repeated text values and
// the non-empty uploaded files (port of collect_multipart).
type multipartForm struct {
	texts map[string][]string
	files []store.Upload
}

func (f multipartForm) first(key string) string {
	if v := f.texts[key]; len(v) > 0 {
		return v[0]
	}
	return ""
}

func (f multipartForm) has(key string) bool {
	_, ok := f.texts[key]
	return ok
}

func (f multipartForm) all(key string) []string { return f.texts[key] }

func isTooLarge(err error) bool {
	return httpx.IsBodyTooLarge(err) || (err != nil && strings.Contains(err.Error(), "request body too large"))
}

// parseMultipart reads the whole multipart body. Writes 413/400 and
// returns false on failure.
func parseMultipart(w http.ResponseWriter, r *http.Request) (multipartForm, bool) {
	err := r.ParseMultipartForm(multipartMemory) //nolint:gosec // G120: bounded by the route BodyLimit
	if err != nil {
		if isTooLarge(err) {
			httpx.TooLarge(w)
			return multipartForm{}, false
		}
		http.Error(w, "Requisição inválida.", http.StatusBadRequest)
		return multipartForm{}, false
	}
	defer func() { _ = r.MultipartForm.RemoveAll() }()
	out := multipartForm{texts: map[string][]string{}}
	for k, v := range r.MultipartForm.Value {
		out.texts[k] = v
	}
	for _, headers := range r.MultipartForm.File {
		for _, fh := range headers {
			up, err := readUpload(fh)
			if err != nil {
				if isTooLarge(err) {
					httpx.TooLarge(w)
					return multipartForm{}, false
				}
				http.Error(w, "Requisição inválida.", http.StatusBadRequest)
				return multipartForm{}, false
			}
			if len(up.Bytes) > 0 {
				out.files = append(out.files, up)
			}
		}
	}
	return out, true
}

func readUpload(fh *multipart.FileHeader) (store.Upload, error) {
	f, err := fh.Open()
	if err != nil {
		return store.Upload{}, err
	}
	defer func() { _ = f.Close() }()
	b := make([]byte, fh.Size)
	if _, err := io.ReadFull(f, b); err != nil {
		return store.Upload{}, err
	}
	ct := fh.Header.Get("Content-Type")
	if ct == "" {
		ct = "application/octet-stream"
	}
	return store.Upload{ContentType: ct, Bytes: b}, nil
}

func parseUUID(raw string) (uuid.UUID, bool) {
	id, err := uuid.Parse(strings.TrimSpace(raw))
	return id, err == nil
}

// failTo sets an error toast and redirects (303, or HX-Location for htmx).
func failTo(w http.ResponseWriter, r *http.Request, location, msg string) {
	toast.Set(w, r, toast.Error(msg))
	toast.Redirect(w, r, location)
}

// okTo sets a success toast and redirects.
func okTo(w http.ResponseWriter, r *http.Request, location, msg string) {
	toast.Set(w, r, toast.Success(msg))
	toast.Redirect(w, r, location)
}

// userMessage is the pt-BR text of a store error, or fallback.
func userMessage(err error, fallback string) string {
	if msg, ok := store.UserMessage(err); ok {
		return msg
	}
	return fallback
}

// fragment renders an htmx fragment and keeps the address bar on the
// equivalent full-page URL.
func fragment(w http.ResponseWriter, r *http.Request, pageURL string, c templ.Component) {
	w.Header().Set(httpx.HXReplaceURL, pageURL)
	render.Fragment(w, r, http.StatusOK, c)
}

// variant maps a store badge tone to the UI kit variant.
func variant(t store.BadgeTone) components.BadgeVariant {
	switch t {
	case store.ToneSecondary:
		return components.BadgeSecondary
	case store.ToneDestructive:
		return components.BadgeDestructive
	case store.ToneOutline:
		return components.BadgeOutline
	}
	return components.BadgePrimary
}

// reais is the Rust "R$ " (cents / 100) whole-real display.
func reais(cents int64) string { return "R$ " + strconv.FormatInt(cents/100, 10) }

func price(cents int64) string { return format.Price(cents) }

func itoa[T ~int | ~int32 | ~int64 | ~int16](n T) string { return strconv.FormatInt(int64(n), 10) }

func deref(s *string) string {
	if s == nil {
		return ""
	}
	return *s
}

// withQuery builds path?k=v… from ordered pairs, skipping empty values.
func withQuery(path string, pairs ...string) string {
	v := url.Values{}
	var keys []string
	for i := 0; i+1 < len(pairs); i += 2 {
		if pairs[i+1] == "" {
			continue
		}
		v.Set(pairs[i], pairs[i+1])
		keys = append(keys, pairs[i])
	}
	if len(keys) == 0 {
		return path
	}
	var b strings.Builder
	for i, k := range keys {
		if i > 0 {
			b.WriteByte('&')
		}
		b.WriteString(url.QueryEscape(k))
		b.WriteByte('=')
		b.WriteString(url.QueryEscape(v.Get(k)))
	}
	return path + "?" + b.String()
}

// pageParam is "" for page 1, else the number.
func pageParam(p int64) string {
	if p <= 1 {
		return ""
	}
	return strconv.FormatInt(p, 10)
}

// formPage parses a hidden page field (positive, else 1).
func formPage(raw string) int64 {
	n, err := strconv.ParseInt(strings.TrimSpace(raw), 10, 64)
	if err != nil || n <= 0 {
		return 1
	}
	return n
}

func sprintf(f string, a ...any) string { return fmt.Sprintf(f, a...) }
