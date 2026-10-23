package storefront

import (
	"log/slog"
	"net/http"
	"net/url"
	"strings"

	"github.com/google/uuid"

	"github.com/wellington-vell/lindaflor/internal/httpx"
	"github.com/wellington-vell/lindaflor/internal/httpx/toast"
	"github.com/wellington-vell/lindaflor/internal/ratelimit"
	"github.com/wellington-vell/lindaflor/internal/store"
	"github.com/wellington-vell/lindaflor/internal/web/render"
)

// stockAlertSubscribe is POST /avise-me (src/app/avise_me.rs subscribe):
// register a back-in-stock alert and go back to the product page with a
// toast. Per-IP budget; a filled honeypot is answered as success.
func (h *handlers) stockAlertSubscribe(w http.ResponseWriter, r *http.Request) {
	if !parseForm(w, r) {
		return
	}
	ctx := r.Context()
	var slug string
	variantID, perr := uuid.Parse(strings.TrimSpace(r.PostForm.Get("variant_id")))
	if perr == nil {
		var err error
		if slug, err = store.VariantSlug(ctx, h.d.Pool, variantID); err != nil {
			serverError(w, r, "variant slug", err)
			return
		}
	}
	if perr != nil || slug == "" {
		toast.Set(w, r, toast.Error("Esta variação não está disponível."))
		toast.Redirect(w, r, "/produtos")
		return
	}
	back := store.ProductVariantPath(slug, variantID)
	if !ratelimit.Check(store.ScopeStockAlert, httpx.ClientIP(r), store.StockAlertSubscribePerMin) {
		toast.Set(w, r, toast.Error("Muitas tentativas. Tente novamente em alguns minutos."))
		toast.Redirect(w, r, back)
		return
	}
	if strings.TrimSpace(r.PostForm.Get(store.HoneypotField)) != "" {
		slog.InfoContext(ctx, "stock alert honeypot tripped; dropped")
		toast.Set(w, r, toast.Success(store.StockAlertSubscribedMessage))
		toast.Redirect(w, r, back)
		return
	}
	email, ok := store.ValidateEmail(r.PostForm.Get("email"))
	if !ok {
		toast.Set(w, r, toast.Error("Informe um e-mail válido."))
		toast.Redirect(w, r, back)
		return
	}
	outcome, err := store.SubscribeStockAlert(ctx, h.d.Pool, variantID, email, userID(r))
	switch {
	case err != nil:
		slog.ErrorContext(ctx, "stock alert subscribe failed", "error", err)
		toast.Set(w, r, toast.Error("Não foi possível registrar o aviso. Tente novamente."))
	default:
		if msg, good := outcome.Toast(); good {
			toast.Set(w, r, toast.Success(msg))
		} else {
			toast.Set(w, r, toast.Error(msg))
		}
	}
	toast.Redirect(w, r, back)
}

// emailLinkView is the cancel/unsubscribe landing page state.
type emailLinkView struct {
	Email, Token string
	Done, Valid  bool
}

// stockAlertCancelPage is GET /avise-me/cancelar: only renders a confirm
// button, so link scanners that prefetch URLs never cancel anything.
func (h *handlers) stockAlertCancelPage(w http.ResponseWriter, r *http.Request) {
	q := r.URL.Query()
	v := emailLinkView{Email: q.Get("email"), Token: q.Get("token"), Done: q.Has("feito")}
	v.Valid = !v.Done && store.VerifyStockAlertCancelToken(v.Email, v.Token)
	render.Page(w, r, h.d, stockAlertCancelView(v)) //nolint:contextcheck // templ components get the request ctx through Render
}

// stockAlertCancel is POST /avise-me/cancelar (the HMAC token is the
// authorization).
func (h *handlers) stockAlertCancel(w http.ResponseWriter, r *http.Request) {
	if !parseForm(w, r) {
		return
	}
	email, token := r.PostForm.Get("email"), r.PostForm.Get("token")
	if !store.VerifyStockAlertCancelToken(email, token) {
		toast.Set(w, r, toast.Error("Link de cancelamento inválido."))
		toast.Redirect(w, r, "/avise-me/cancelar")
		return
	}
	if _, err := store.CancelStockAlerts(r.Context(), h.d.Pool, email); err != nil {
		slog.ErrorContext(r.Context(), "stock alert cancel failed", "error", err)
		toast.Set(w, r, toast.Error("Não foi possível cancelar os avisos. Tente novamente."))
		toast.Redirect(w, r, "/")
		return
	}
	toast.Redirect(w, r, "/avise-me/cancelar?"+url.Values{"feito": {"1"}}.Encode())
}
