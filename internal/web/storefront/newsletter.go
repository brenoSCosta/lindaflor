package storefront

import (
	"log/slog"
	"net/http"

	"github.com/wellington-vell/lindaflor/internal/httpx"
	"github.com/wellington-vell/lindaflor/internal/httpx/toast"
	"github.com/wellington-vell/lindaflor/internal/ratelimit"
	"github.com/wellington-vell/lindaflor/internal/store"
	"github.com/wellington-vell/lindaflor/internal/web/render"
)

// newsletterSubscribe is POST /newsletter (src/app/newsletter.rs
// subscribe): 5 per minute per IP, then back to the home page with a toast.
func (h *handlers) newsletterSubscribe(w http.ResponseWriter, r *http.Request) {
	if !parseForm(w, r) {
		return
	}
	home := func() { toast.Redirect(w, r, "/") }
	if !ratelimit.Check(store.ScopeNewsletter, httpx.ClientIP(r), store.NewsletterPerMin) {
		toast.Set(w, r, toast.Error("Muitas tentativas. Tente novamente em alguns minutos."))
		home()
		return
	}
	email, name, msg := store.ValidateNewsletterForm(r.PostForm.Get("email"), r.PostForm.Get("name"))
	if msg != "" {
		toast.Set(w, r, toast.Error(msg))
		home()
		return
	}
	outcome, err := store.SubscribeNewsletter(r.Context(), h.d.Pool, email, name)
	if err != nil {
		slog.ErrorContext(r.Context(), "newsletter subscribe failed", "error", err)
		toast.Set(w, r, toast.Error("Não foi possível concluir a inscrição. Tente novamente."))
		home()
		return
	}
	if text, good := outcome.Toast(); good {
		toast.Set(w, r, toast.Success(text))
	} else {
		toast.Set(w, r, toast.Error(text))
	}
	home()
}

// unsubscribePage is GET /newsletter/cancelar: a confirm button only, so
// link scanners never unsubscribe anyone.
func (h *handlers) unsubscribePage(w http.ResponseWriter, r *http.Request) {
	q := r.URL.Query()
	v := emailLinkView{Email: q.Get("email"), Token: q.Get("token"), Done: q.Has("feito")}
	v.Valid = !v.Done && store.VerifyUnsubscribeToken(v.Email, v.Token)
	render.Page(w, r, h.d, unsubscribeView(v)) //nolint:contextcheck // templ components get the request ctx through Render
}

// unsubscribe is POST /newsletter/cancelar: the confirm form, or a mail
// client's RFC 8058 one-click POST (List-Unsubscribe=One-Click body,
// email/token in the query string). Exempt from the origin check
// (httpx.OriginExemptPaths); the HMAC token is the authorization.
func (h *handlers) unsubscribe(w http.ResponseWriter, r *http.Request) {
	if !parseForm(w, r) {
		return
	}
	q := r.URL.Query()
	_, oneClick := r.PostForm["List-Unsubscribe"]
	email := firstOf(r.PostForm, q, "email")
	token := firstOf(r.PostForm, q, "token")
	if !store.VerifyUnsubscribeToken(email, token) {
		if oneClick {
			http.Error(w, "invalid token", http.StatusBadRequest)
			return
		}
		toast.Set(w, r, toast.Error("Link de cancelamento inválido."))
		toast.Redirect(w, r, "/newsletter/cancelar")
		return
	}
	if _, err := store.UnsubscribeNewsletter(r.Context(), h.d.Pool, email); err != nil {
		slog.ErrorContext(r.Context(), "newsletter unsubscribe failed", "error", err)
		if oneClick {
			http.Error(w, "retry", http.StatusServiceUnavailable)
			return
		}
		toast.Set(w, r, toast.Error("Não foi possível cancelar a inscrição. Tente novamente."))
		toast.Redirect(w, r, "/")
		return
	}
	if oneClick {
		w.Header().Set("Content-Type", "text/plain; charset=utf-8")
		_, _ = w.Write([]byte("unsubscribed"))
		return
	}
	toast.Redirect(w, r, "/newsletter/cancelar?feito=1")
}

// firstOf is the body value of key when present, else the query value.
func firstOf(body, query map[string][]string, key string) string {
	if vs, ok := body[key]; ok && len(vs) > 0 {
		return vs[0]
	}
	if vs, ok := query[key]; ok && len(vs) > 0 {
		return vs[0]
	}
	return ""
}
