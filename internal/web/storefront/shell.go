package storefront

import (
	"net/http"

	"github.com/wellington-vell/lindaflor/internal/app"
	"github.com/wellington-vell/lindaflor/internal/auth"
	"github.com/wellington-vell/lindaflor/internal/store"
	"github.com/wellington-vell/lindaflor/internal/web/components"
	"github.com/wellington-vell/lindaflor/internal/web/layout"
)

// ShellLoader fills layout.ShellData for every full page render
// (src/app/app_sidebar.rs storefront_shell / app_shell): store settings
// (announcement + footer), the first six active sets (header menu), the
// cart badge (cookie token, never creates a cart), the current user, the
// presigned avatar and the sidebar_state cookie (app shell). The cart sheet
// body is loaded by htmx when the sheet opens (GET /carrinho/sheet).
func ShellLoader(d *app.Deps) app.ShellLoader {
	return func(r *http.Request) (layout.ShellData, error) {
		ctx := r.Context()
		s := layout.ShellData{Settings: layout.DefaultStoreSettings(), SidebarExpanded: true}
		if u, ok := auth.CurrentUser(ctx); ok {
			s.User = u
		}
		if c, err := r.Cookie(components.SidebarCookie); err == nil && c.Value == "collapsed" {
			s.SidebarExpanded = false
		}
		if d.Pool == nil {
			return s, nil
		}
		if s.User != nil && layout.IsAppShellPath(r.URL.Path) {
			// The app shell shows the avatar; the storefront data is unused.
			if d.Storage != nil {
				s.AvatarURL = auth.ResolveAvatarURL(ctx, d.Storage, s.User.ID, s.User.Image)
			}
			return s, nil
		}
		st := store.StorefrontSettings(ctx, d.Pool)
		s.Settings = layout.StoreSettings{
			PixDiscountPercent:         st.PixDiscountPercent,
			FreeShippingThresholdCents: st.FreeShippingThresholdCents,
			AnnouncementText:           deref(st.AnnouncementText),
			WhatsappNumber:             st.WhatsappNumberTrimmed(),
		}
		var firstErr error
		if sets, err := store.NavSets(ctx, d.Pool); err != nil {
			firstErr = err
		} else {
			for _, set := range sets {
				s.Sets = append(s.Sets, layout.NavSet{Slug: set.Slug, Name: set.Name})
			}
		}
		secure := d.Cfg != nil && d.Cfg.SecureCookies
		if n, err := store.CartCountHTTP(r, d.Pool, secure); err != nil {
			if firstErr == nil {
				firstErr = err
			}
		} else {
			s.CartCount = n
		}
		return s, firstErr
	}
}
