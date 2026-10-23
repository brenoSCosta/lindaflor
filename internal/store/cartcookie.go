package store

import (
	"net/http"
	"strings"
	"time"

	"github.com/google/uuid"
	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/wellington-vell/lindaflor/internal/auth"
	"github.com/wellington-vell/lindaflor/internal/db"
)

// The cart bearer-token cookie (src/app/store/cart.rs write_cart_token /
// clear_cart_token): "lindaflor_cart" in development,
// "__Host-lindaflor_cart" with Secure cookies (the prefix keeps a network
// attacker from planting a sibling-domain cart token). HttpOnly,
// SameSite=Lax, Path=/, 30 days.

// CartCookieMaxAge is the cart cookie lifetime (also the guest-cart sweep
// horizon in internal/maintenance).
const CartCookieMaxAge = 30 * 24 * time.Hour

// CartCookieName is the cart cookie for the Secure-cookies setting.
func CartCookieName(secure bool) string {
	if secure {
		return auth.CartCookieSecure
	}
	return auth.CartCookieDev
}

// CartToken reads and parses the cart cookie (nil when absent/invalid).
func CartToken(r *http.Request, secure bool) *uuid.UUID {
	c, err := r.Cookie(CartCookieName(secure))
	if err != nil || c.Value == "" {
		return nil
	}
	token, err := uuid.Parse(strings.TrimSpace(c.Value))
	if err != nil {
		return nil
	}
	return &token
}

// WriteCartToken (re)sets the cart cookie.
func WriteCartToken(w http.ResponseWriter, token uuid.UUID, secure bool) {
	auth.SetCookie(w, CartCookieName(secure), token.String(), CartCookieMaxAge, secure)
}

// ClearCartToken expires the cart cookie (logout).
func ClearCartToken(w http.ResponseWriter, secure bool) {
	auth.ClearCookie(w, CartCookieName(secure), secure)
}

// FindCartHTTP is FindCart for a request: reads the cookie and rewrites it
// when the user's cart was found without one.
func FindCartHTTP(w http.ResponseWriter, r *http.Request, q DBTX, secure bool, userID *uuid.UUID) (*CartRef, error) {
	ref, write, err := FindCart(r.Context(), q, CartToken(r, secure), userID)
	if err == nil && write && ref != nil {
		WriteCartToken(w, ref.Token, secure)
	}
	return ref, err
}

// LoadOrCreateCartHTTP is LoadOrCreateCart for a request (writes the cookie
// for a new/re-attached cart).
func LoadOrCreateCartHTTP(w http.ResponseWriter, r *http.Request, q DBTX, secure bool, userID *uuid.UUID) (CartRef, error) {
	ref, write, err := LoadOrCreateCart(r.Context(), q, CartToken(r, secure), userID)
	if err == nil && write {
		WriteCartToken(w, ref.Token, secure)
	}
	return ref, err
}

// CartCountHTTP is the header badge for a request (no cart is created).
func CartCountHTTP(r *http.Request, q DBTX, secure bool) (int, error) {
	return CartItemCount(r.Context(), q, CartToken(r, secure))
}

// MergeGuestCartOnLogin merges the guest cart of the request's cookie into
// userID's cart and points the cookie at the surviving cart.
func MergeGuestCartOnLogin(w http.ResponseWriter, r *http.Request, pool *pgxpool.Pool, userID uuid.UUID, secure bool) error {
	var token uuid.UUID
	ctx := r.Context()
	err := db.WithTx(ctx, pool, func(tx pgx.Tx) error {
		var err error
		token, err = MergeGuestCart(ctx, tx, CartToken(r, secure), userID)
		return err
	})
	if err != nil {
		return err
	}
	WriteCartToken(w, token, secure)
	return nil
}

// secureFromLogin infers the Secure-cookies mode of a login response: the
// session cookie auth just wrote is __Host- prefixed exactly when cookies
// are Secure (TLS requests count too).
func secureFromLogin(w http.ResponseWriter, r *http.Request) bool {
	for _, line := range w.Header().Values("Set-Cookie") {
		if strings.HasPrefix(line, "__Host-") {
			return true
		}
	}
	return r.TLS != nil
}

// cartLoginHook is registered with auth: every regular login merges the
// guest cart and re-issues the cart cookie (Rust start_session_for_user).
func cartLoginHook(w http.ResponseWriter, r *http.Request, pool *pgxpool.Pool, userID uuid.UUID) error {
	return MergeGuestCartOnLogin(w, r, pool, userID, secureFromLogin(w, r))
}

func init() { auth.RegisterLoginHook(cartLoginHook) }
