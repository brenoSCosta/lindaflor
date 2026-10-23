package httpx

import (
	"context"
	"fmt"
	"net"
	"net/http"
	"net/netip"
	"slices"
	"strings"
)

// TrustedProxies resolves the client address from the peer and
// X-Forwarded-For (port of Topcoat's TrustedProxies, XFF mode).
//
// Walking outward from the direct connection, the first untrusted address
// is the client. A hop is trusted by position (Nearest: the first N hops
// out from the app, the peer being hop 0) or by network. With the zero
// value nothing is trusted: the peer is the client and proxy headers are
// ignored, so spoofed X-Forwarded-For values cannot reset rate-limit or
// lockout buckets.
type TrustedProxies struct {
	Nearest  int
	Networks []netip.Prefix
}

// NewTrustedProxies parses TRUSTED_PROXY_NETWORKS entries (CIDRs or IPs).
func NewTrustedProxies(nearest int, networks []string) (TrustedProxies, error) {
	tp := TrustedProxies{Nearest: nearest}
	for _, n := range networks {
		if p, err := netip.ParsePrefix(n); err == nil {
			tp.Networks = append(tp.Networks, p.Masked())
			continue
		}
		a, err := netip.ParseAddr(n)
		if err != nil {
			return tp, fmt.Errorf("invalid trusted proxy network %q", n)
		}
		a = a.Unmap()
		tp.Networks = append(tp.Networks, netip.PrefixFrom(a, a.BitLen()))
	}
	return tp, nil
}

func (tp TrustedProxies) trusted(addr netip.Addr, ok bool, hop int) bool {
	if hop < tp.Nearest {
		return true
	}
	return ok && slices.ContainsFunc(tp.Networks, func(p netip.Prefix) bool { return p.Contains(addr) })
}

// ClientIP resolves the client address; ok=false when it cannot be named.
// remote is the peer address (ok=false for e.g. a Unix socket peer).
func (tp TrustedProxies) ClientIP(remote netip.Addr, remoteOK bool, xff []string) (netip.Addr, bool) {
	if remoteOK {
		remote = remote.Unmap()
	}
	if !tp.trusted(remote, remoteOK, 0) {
		return remote, remoteOK
	}
	// Entries from the one nearest to the application outward.
	var entries []string
	for _, v := range slices.Backward(xff) {
		parts := strings.Split(v, ",")
		for _, p := range slices.Backward(parts) {
			if p = strings.TrimSpace(p); p != "" {
				entries = append(entries, p)
			}
		}
	}
	farthest, farthestOK := remote, remoteOK
	for i, e := range entries {
		hop := i + 1
		addr, ok := parseNode(e)
		if hop >= tp.Nearest {
			if !ok {
				return netip.Addr{}, false
			}
			if !tp.trusted(addr, true, hop) {
				return addr, true
			}
		}
		farthest, farthestOK = addr, ok
	}
	return farthest, farthestOK
}

// parseNode reads an address, stripping a port and IPv6 brackets.
func parseNode(s string) (netip.Addr, bool) {
	s = strings.TrimSpace(s)
	if a, err := netip.ParseAddr(s); err == nil {
		return a.Unmap(), true
	}
	if ap, err := netip.ParseAddrPort(s); err == nil {
		return ap.Addr().Unmap(), true
	}
	if strings.HasPrefix(s, "[") && strings.HasSuffix(s, "]") {
		if a, err := netip.ParseAddr(s[1 : len(s)-1]); err == nil {
			return a.Unmap(), true
		}
	}
	return netip.Addr{}, false
}

func remoteAddr(r *http.Request) (netip.Addr, bool) {
	host, _, err := net.SplitHostPort(r.RemoteAddr)
	if err != nil {
		host = r.RemoteAddr
	}
	a, err := netip.ParseAddr(host)
	if err != nil {
		return netip.Addr{}, false
	}
	return a.Unmap(), true
}

// ClientIPMiddleware resolves the client IP once per request.
func ClientIPMiddleware(tp TrustedProxies) func(http.Handler) http.Handler {
	return func(next http.Handler) http.Handler {
		return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			remote, ok := remoteAddr(r)
			ip, ipOK := tp.ClientIP(remote, ok, r.Header.Values("X-Forwarded-For"))
			s := "unknown"
			if ipOK {
				s = ip.String()
			}
			next.ServeHTTP(w, r.WithContext(context.WithValue(r.Context(), clientIPKey, s)))
		})
	}
}

// ClientIP is the resolved client IP ("unknown" when it cannot be named).
// Without ClientIPMiddleware it falls back to the peer address.
func ClientIP(r *http.Request) string {
	if s, ok := r.Context().Value(clientIPKey).(string); ok {
		return s
	}
	if a, ok := remoteAddr(r); ok {
		return a.String()
	}
	return "unknown"
}

// UserAgent is the request User-Agent ("" when absent).
func UserAgent(r *http.Request) string { return r.Header.Get("User-Agent") }
