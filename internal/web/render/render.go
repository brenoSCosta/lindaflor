// Package render writes templ pages and fragments.
//
//	render.Page(w, r, d, pages.Home(data))       // full page in the right shell
//	render.Fragment(w, r, components.CartLines(x)) // htmx fragment
//	render.Status(w, r, d, http.StatusUnprocessableEntity, page) // e.g. form errors
//
// Multi-target htmx updates: wrap extra regions in components.HXPartial.
//
// Page answers htmx "partial" requests with just the page component, and
// full requests (plain or HX-Request-Type: full) with the root layout.
package render

import (
	"log/slog"
	"net/http"

	"github.com/a-h/templ"

	"github.com/wellington-vell/lindaflor/internal/app"
	"github.com/wellington-vell/lindaflor/internal/auth"
	"github.com/wellington-vell/lindaflor/internal/config"
	"github.com/wellington-vell/lindaflor/internal/httpx"
	"github.com/wellington-vell/lindaflor/internal/httpx/toast"
	"github.com/wellington-vell/lindaflor/internal/seo"
	"github.com/wellington-vell/lindaflor/internal/web/layout"
)

// Page renders content inside the root layout with status 200.
func Page(w http.ResponseWriter, r *http.Request, d *app.Deps, content templ.Component) {
	Status(w, r, d, http.StatusOK, content)
}

// Status is Page with an explicit status code.
func Status(w http.ResponseWriter, r *http.Request, d *app.Deps, status int, content templ.Component) {
	noStoreInDev(w, d)
	if httpx.IsPartial(r) {
		Fragment(w, r, status, content)
		return
	}
	page(w, r, d, status, content, false)
}

// NotFound renders the branded 404 page (never indexed).
func NotFound(w http.ResponseWriter, r *http.Request, d *app.Deps) {
	page(w, r, d, http.StatusNotFound, layout.NotFound(), true)
}

// Fragment renders a component alone (htmx swaps, <hx-partial> bundles).
func Fragment(w http.ResponseWriter, r *http.Request, status int, c templ.Component) {
	w.Header().Set("Content-Type", "text/html; charset=utf-8")
	w.WriteHeader(status)
	if err := c.Render(r.Context(), w); err != nil {
		slog.ErrorContext(r.Context(), "render fragment", "error", err, "path", r.URL.Path)
	}
}

// noStoreInDev keeps the browser from reusing an old HTML document (and its
// immutable stylesheet URL) after a live-reload rebuild. Production keeps the
// default caching behavior.
func noStoreInDev(w http.ResponseWriter, d *app.Deps) {
	if d == nil || d.Cfg == nil || !config.IsDevEnv(d.Cfg.AppEnv) {
		return
	}
	w.Header().Set("Cache-Control", "no-store")
}

func page(w http.ResponseWriter, r *http.Request, d *app.Deps, status int, content templ.Component, failed bool) {
	ctx := r.Context()
	noStoreInDev(w, d)
	var shell layout.ShellData
	if d.Shell != nil {
		s, err := d.Shell(r)
		if err != nil {
			slog.ErrorContext(ctx, "load shell data", "error", err)
		}
		shell = s
	} else {
		shell.Settings = layout.DefaultStoreSettings()
	}
	if u, ok := auth.CurrentUser(ctx); ok && shell.User == nil {
		shell.User = u
	}
	shell.Impersonating = shell.Impersonating || auth.IsImpersonating(ctx)

	toasts := toast.Take(w, r)
	if httpx.IsHTMX(r) {
		// The toaster is hx-preserve'd: deliver flashes on the header.
		toast.EmitHeader(w, r, toasts)
		toasts = nil
	}
	head, set := seo.HeadFrom(ctx)
	appOrigin := ""
	if d.Cfg != nil {
		appOrigin = d.Cfg.AppOrigin
	}
	p := layout.Page{
		Shell:    shell,
		Head:     seo.Resolve(head, set, failed, r.URL.Path, seo.SiteOrigin(appOrigin, r)),
		Toasts:   toasts,
		Path:     r.URL.Path,
		AppShell: shell.User != nil && layout.IsAppShellPath(r.URL.Path),
	}
	if d.Cfg != nil && config.IsDevEnv(d.Cfg.AppEnv) {
		p.DevVersion = app.BuildID
	}
	w.Header().Set("Content-Type", "text/html; charset=utf-8")
	w.WriteHeader(status)
	if err := layout.Root(p).Render(templ.WithChildren(ctx, content), w); err != nil {
		slog.ErrorContext(ctx, "render page", "error", err, "path", r.URL.Path)
	}
}
