package httpx

import (
	"net/http"
	"net/url"
)

// htmx 4 request headers (see docs/htmx4-notes.md).
const (
	HXRequest     = "HX-Request"      // always "true" on htmx requests
	HXRequestType = "HX-Request-Type" // "partial" or "full" (target is body / hx-select)
	HXCurrentURL  = "HX-Current-URL"  // the page URL the request came from
	HXBoosted     = "HX-Boosted"      // hx-boost navigation
	HXTarget      = "HX-Target"       // "tag#id" of the swap target
	HXSource      = "HX-Source"       // "tag#id" of the triggering element
	HXHistory     = "HX-History-Restore-Request"
)

// htmx response headers.
const (
	HXLocation   = "HX-Location"
	HXRedirect   = "HX-Redirect"
	HXRefresh    = "HX-Refresh"
	HXRetarget   = "HX-Retarget"
	HXReswap     = "HX-Reswap"
	HXReselect   = "HX-Reselect"
	HXTrigger    = "HX-Trigger"
	HXPushURL    = "HX-Push-Url"
	HXReplaceURL = "HX-Replace-Url"
)

// IsHTMX reports an htmx-issued request (HX-Request: true).
func IsHTMX(r *http.Request) bool { return r.Header.Get(HXRequest) == "true" }

// IsPartial reports an htmx request whose target is not the whole body:
// shard/fragment endpoints render just their fragment for these.
func IsPartial(r *http.Request) bool {
	return IsHTMX(r) && r.Header.Get(HXRequestType) != "full"
}

// CurrentPath is the path of the page the user is looking at: the
// HX-Current-URL path for htmx requests, else the request path.
func CurrentPath(r *http.Request) string {
	if cur := r.Header.Get(HXCurrentURL); cur != "" && IsHTMX(r) {
		if u, err := url.Parse(cur); err == nil && u.Path != "" {
			return u.Path
		}
	}
	return r.URL.Path
}

// Redirect sends the browser to a same-site location. Regular requests
// get 303 See Other (POST-redirect-GET, works without JS). htmx requests
// get 200 + HX-Location so htmx performs a client-side navigation (a 3xx
// would be followed by fetch and swapped into the target instead).
func Redirect(w http.ResponseWriter, r *http.Request, location string) {
	if IsHTMX(r) {
		w.Header().Set(HXLocation, location)
		w.WriteHeader(http.StatusOK)
		return
	}
	http.Redirect(w, r, location, http.StatusSeeOther)
}
