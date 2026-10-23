// Package web assembles the HTTP router: the middleware stack plus one
// Mount call per feature package. Feature work never edits this file —
// each package owns its routes in its own Mount.
package web

import (
	"fmt"
	"net/http"

	"github.com/go-chi/chi/v5"

	"github.com/wellington-vell/lindaflor/internal/app"
	"github.com/wellington-vell/lindaflor/internal/auth"
	"github.com/wellington-vell/lindaflor/internal/httpx"
	"github.com/wellington-vell/lindaflor/internal/httpx/toast"
	"github.com/wellington-vell/lindaflor/internal/logging"
	"github.com/wellington-vell/lindaflor/internal/metrics"
	"github.com/wellington-vell/lindaflor/internal/ratelimit"
	"github.com/wellington-vell/lindaflor/internal/seo"
	"github.com/wellington-vell/lindaflor/internal/web/account"
	"github.com/wellington-vell/lindaflor/internal/web/admin"
	"github.com/wellington-vell/lindaflor/internal/web/authapi"
	"github.com/wellington-vell/lindaflor/internal/web/authpages"
	"github.com/wellington-vell/lindaflor/internal/web/checkout"
	"github.com/wellington-vell/lindaflor/internal/web/platform"
	"github.com/wellington-vell/lindaflor/internal/web/render"
	"github.com/wellington-vell/lindaflor/internal/web/storefront"
	"github.com/wellington-vell/lindaflor/web/static"
)

// NewRouter builds the application handler. d.Cfg, d.Logger must be set;
// d.Shell defaults to the storefront loader.
func NewRouter(d *app.Deps) (http.Handler, error) {
	if d.Cfg == nil || d.Logger == nil {
		return nil, fmt.Errorf("web: Deps.Cfg and Deps.Logger are required")
	}
	if d.Shell == nil {
		d.Shell = storefront.ShellLoader(d)
	}
	proxies, err := httpx.NewTrustedProxies(d.Cfg.TrustedProxyHops, d.Cfg.TrustedProxyNets)
	if err != nil {
		return nil, err
	}
	origin, err := httpx.OriginProtection(d.Cfg.AppOrigin)
	if err != nil {
		return nil, fmt.Errorf("APP_ORIGIN: %w", err)
	}

	r := chi.NewRouter()
	r.Use(
		httpx.RequestID,
		httpx.ClientIPMiddleware(proxies),
		logging.Middleware(d.Logger),
		httpx.Recover,
		httpx.SecurityHeaders(d.Cfg.AppEnv),
		metrics.Middleware,
		origin,
		httpx.GlobalBodyLimit(httpx.DefaultBodyLimit),
		toast.Middleware(d.Cfg.SecureCookies),
		seo.Middleware,
		auth.Middleware(auth.MiddlewareDeps{Pool: d.Pool, Cfg: d.Cfg, Valkey: d.Valkey}),
		ratelimit.Global(),
	)

	r.Handle(static.Prefix+"*", static.Handler())
	r.Get("/favicon.ico", static.Favicon)
	r.Head("/favicon.ico", static.Favicon)

	platform.Mount(r, d)
	authapi.Mount(r, d)
	authpages.Mount(r, d)
	storefront.Mount(r, d)
	checkout.Mount(r, d)
	account.Mount(r, d)
	admin.Mount(r, d)

	r.NotFound(func(w http.ResponseWriter, req *http.Request) { render.NotFound(w, req, d) })
	r.MethodNotAllowed(func(w http.ResponseWriter, _ *http.Request) {
		http.Error(w, "Método não permitido.", http.StatusMethodNotAllowed)
	})
	return r, nil
}
