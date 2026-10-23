package auth

import (
	"context"
	"crypto/sha256"
	"encoding/base64"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"log/slog"
	"net/http"
	"net/url"
	"strings"
	"time"

	"github.com/google/uuid"
	"github.com/jackc/pgx/v5"

	"github.com/wellington-vell/lindaflor/internal/db"
	"github.com/wellington-vell/lindaflor/internal/db/sqlc"
	"github.com/wellington-vell/lindaflor/internal/httpx"
)

// GoogleProviderID is accounts.provider_id of Google logins.
const GoogleProviderID = "google"

const (
	googleAuthURL     = "https://accounts.google.com/o/oauth2/v2/auth"
	googleTokenURL    = "https://oauth2.googleapis.com/token" //nolint:gosec // G101: public endpoint, not a credential
	googleUserinfoURL = "https://openidconnect.googleapis.com/v1/userinfo"
	oauthScopes       = "openid email profile"
	// DefaultCallbackPath is where OAuth flows land without a safe callback.
	DefaultCallbackPath = "/dashboard"
)

// GoogleOAuthConfig is the configured Google client.
type GoogleOAuthConfig struct {
	ClientID     string
	ClientSecret string
	AppOrigin    string
	// TestBase replaces Google's token/userinfo host (dev/test only).
	TestBase string
}

// GoogleConfig returns the Google client when GOOGLE_CLIENT_ID,
// GOOGLE_CLIENT_SECRET and APP_ORIGIN are all set.
func (s *Service) GoogleConfig() (*GoogleOAuthConfig, bool) {
	if !s.Cfg.Google.Enabled() || s.Cfg.AppOrigin == "" {
		return nil, false
	}
	return &GoogleOAuthConfig{
		ClientID: s.Cfg.Google.ClientID, ClientSecret: s.Cfg.Google.ClientSecret,
		AppOrigin: s.Cfg.AppOrigin, TestBase: s.Cfg.GoogleOAuthTestBase,
	}, true
}

// GoogleNotConfiguredMessage explains which settings are missing (501 body).
func (s *Service) GoogleNotConfiguredMessage() string {
	if s.Cfg.Google.Enabled() {
		return "Google sign-in is not configured (set APP_ORIGIN, WEB_ORIGIN, or BETTER_AUTH_URL)"
	}
	return "Google sign-in is not configured (set GOOGLE_CLIENT_ID and GOOGLE_CLIENT_SECRET)"
}

// RedirectURI is the registered callback.
func (c *GoogleOAuthConfig) RedirectURI() string { return c.AppOrigin + "/api/auth/callback/google" }

// AuthorizeURL builds Google's authorize URL with an optional PKCE S256
// challenge. No OIDC nonce: identity comes from userinfo over the
// server-side PKCE-bound exchange and the id_token is never parsed.
func (c *GoogleOAuthConfig) AuthorizeURL(state, codeChallenge string) string {
	v := url.Values{}
	v.Set("client_id", c.ClientID)
	v.Set("redirect_uri", c.RedirectURI())
	v.Set("response_type", "code")
	v.Set("scope", oauthScopes)
	v.Set("state", state)
	v.Set("access_type", "online")
	v.Set("prompt", "select_account")
	if codeChallenge != "" {
		v.Set("code_challenge", codeChallenge)
		v.Set("code_challenge_method", "S256")
	}
	return googleAuthURL + "?" + v.Encode()
}

func (c *GoogleOAuthConfig) tokenURL() string {
	if c.TestBase != "" {
		return c.TestBase + "/token"
	}
	return googleTokenURL
}

func (c *GoogleOAuthConfig) userinfoURL() string {
	if c.TestBase != "" {
		return c.TestBase + "/userinfo"
	}
	return googleUserinfoURL
}

// PKCES256Challenge is base64url(sha256(verifier)) without padding (RFC 7636).
func PKCES256Challenge(verifier string) string {
	sum := sha256.Sum256([]byte(verifier))
	return base64.RawURLEncoding.EncodeToString(sum[:])
}

// SafeCallbackPath keeps only local redirects (default /dashboard).
func SafeCallbackPath(callback string) string {
	return httpx.SafeLocalRedirectOr(callback, DefaultCallbackPath)
}

// GoogleEmailVerified: a missing claim is unverified (never default true).
func GoogleEmailVerified(claim *bool) bool { return claim != nil && *claim }

// ErrUnverifiedGoogleEmail aborts linking and creating alike.
const unverifiedGoogleEmailMsg = "please verify your Google email first, then try again"

// GoogleEmailSignupAction decides what a Google email does.
type GoogleEmailSignupAction int

// Signup actions.
const (
	LinkExisting GoogleEmailSignupAction = iota
	CreateNew
)

// DecideGoogleEmailSignup: unverified aborts both linking and creating (a
// later unique-email collision must not take over the victim row).
func DecideGoogleEmailSignup(emailVerified, existingUser bool) (GoogleEmailSignupAction, error) {
	if !emailVerified {
		return 0, BadRequest(unverifiedGoogleEmailMsg)
	}
	if existingUser {
		return LinkExisting, nil
	}
	return CreateNew, nil
}

// VerifyLinkSession: a link-flow callback must run in the session (token
// hash) and for the user that started it.
func VerifyLinkSession(expectedBinding, actualHash string, linkUserID uuid.UUID, actualUserID *uuid.UUID) error {
	expectedBinding, actualHash = strings.TrimSpace(expectedBinding), strings.TrimSpace(actualHash)
	if expectedBinding == "" {
		return BadRequest("OAuth state is not bound to a session; restart linking")
	}
	if actualHash == "" || expectedBinding != actualHash {
		return BadRequest("OAuth session changed during linking; sign in and try again")
	}
	if actualUserID == nil || *actualUserID != linkUserID {
		return BadRequest("OAuth link account mismatch; sign in and try again")
	}
	return nil
}

// VerifyBrowserBinding: the callback must present the oauth_bind cookie of
// the browser that started the flow (login CSRF guard).
func VerifyBrowserBinding(expected, actual string) error {
	expected, actual = strings.TrimSpace(expected), strings.TrimSpace(actual)
	if expected == "" || actual == "" || expected != actual {
		return BadRequest("OAuth flow started in another browser; sign in again")
	}
	return nil
}

func oauthBindHash(v string) string {
	sum := sha256.Sum256([]byte(v))
	return hex.EncodeToString(sum[:])
}

func (s *Service) bindCookieName() string {
	return cookieName(s.secure(), OAuthBindCookieDev, OAuthBindCookieSecure)
}

// issueBrowserBinding sets a fresh oauth_bind cookie (10 min) and returns
// its hash for the state row.
func (s *Service) issueBrowserBinding(w http.ResponseWriter) string {
	v := RandomToken()
	SetCookie(w, s.bindCookieName(), v, OAuthStateTTL, s.secure())
	return oauthBindHash(v)
}

// takeBrowserBinding returns the presented cookie hash and clears it.
func (s *Service) takeBrowserBinding(w http.ResponseWriter, r *http.Request) string {
	v := readCookie(r, s.bindCookieName())
	ClearCookie(w, s.bindCookieName(), s.secure())
	if v == "" {
		return ""
	}
	return oauthBindHash(v)
}

type oauthStateValue struct {
	Callback       string     `json:"callback"`
	LinkUserID     *uuid.UUID `json:"linkUserId,omitempty"`
	SessionBinding *string    `json:"sessionBinding,omitempty"`
	BrowserBinding *string    `json:"browserBinding,omitempty"`
	CodeVerifier   *string    `json:"codeVerifier,omitempty"`
}

// StartGoogleOAuth stores a PKCE + browser-bound state row (10 min) and
// returns the authorize URL. linkUserID + sessionBinding (current session
// token hash) start a link flow.
func (s *Service) StartGoogleOAuth(w http.ResponseWriter, r *http.Request, callbackURL string, linkUserID *uuid.UUID, sessionBinding string) (string, error) {
	cfg, ok := s.GoogleConfig()
	if !ok {
		return "", BadRequest("Google sign-in is not configured")
	}
	if linkUserID != nil && strings.TrimSpace(sessionBinding) == "" {
		return "", BadRequest("OAuth link requires an active session")
	}
	state, verifier := RandomToken(), RandomToken()
	bind := s.issueBrowserBinding(w)
	val := oauthStateValue{Callback: SafeCallbackPath(callbackURL), LinkUserID: linkUserID, BrowserBinding: &bind, CodeVerifier: &verifier}
	if linkUserID != nil {
		val.SessionBinding = &sessionBinding
	}
	raw, err := json.Marshal(val)
	if err != nil {
		return "", err
	}
	if err := s.Q.AuthInsertVerification(r.Context(), sqlc.AuthInsertVerificationParams{
		ID: newID(), Identifier: OAuthStatePrefix + state, Value: string(raw), ExpiresAt: time.Now().Add(OAuthStateTTL),
	}); err != nil {
		return "", err
	}
	return cfg.AuthorizeURL(state, PKCES256Challenge(verifier)), nil
}

var errInvalidOAuthState = BadRequest("invalid or expired OAuth state")

func (s *Service) consumeOAuthState(ctx context.Context, state string) (*oauthStateValue, error) {
	row, err := s.Q.AuthVerificationByIdentifier(ctx, OAuthStatePrefix+state)
	if errors.Is(err, pgx.ErrNoRows) {
		return nil, errInvalidOAuthState
	}
	if err != nil {
		return nil, err
	}
	if err := s.Q.AuthDeleteVerification(ctx, row.ID); err != nil {
		return nil, err
	}
	var v oauthStateValue
	if err := json.Unmarshal([]byte(row.Value), &v); err != nil {
		return nil, errInvalidOAuthState
	}
	v.Callback = SafeCallbackPath(v.Callback)
	return &v, nil
}

type googleTokens struct {
	AccessToken string  `json:"access_token"`
	Scope       *string `json:"scope"`
}

type googleUserInfo struct {
	Sub           string  `json:"sub"`
	Email         *string `json:"email"`
	EmailVerified *bool   `json:"email_verified"`
	Name          *string `json:"name"`
	Picture       *string `json:"picture"`
}

func (s *Service) googleJSON(req *http.Request, out any, what string) error {
	req.Header.Set("Accept", "application/json")
	res, err := s.HTTP.Do(req)
	if err != nil {
		return fmt.Errorf("google %s: %w", what, err)
	}
	defer func() { _ = res.Body.Close() }()
	body, err := io.ReadAll(io.LimitReader(res.Body, 1<<20))
	if err != nil {
		return fmt.Errorf("google %s: %w", what, err)
	}
	if res.StatusCode < 200 || res.StatusCode > 299 {
		return fmt.Errorf("google %s failed: status %d", what, res.StatusCode)
	}
	if err := json.Unmarshal(body, out); err != nil {
		return fmt.Errorf("google %s: %w", what, err)
	}
	return nil
}

func (s *Service) exchangeCode(ctx context.Context, cfg *GoogleOAuthConfig, code, verifier string) (*googleTokens, error) {
	form := url.Values{
		"code": {code}, "client_id": {cfg.ClientID}, "client_secret": {cfg.ClientSecret},
		"redirect_uri": {cfg.RedirectURI()}, "grant_type": {"authorization_code"}, "code_verifier": {verifier},
	}
	req, err := http.NewRequestWithContext(ctx, http.MethodPost, cfg.tokenURL(), strings.NewReader(form.Encode()))
	if err != nil {
		return nil, err
	}
	req.Header.Set("Content-Type", "application/x-www-form-urlencoded")
	var t googleTokens
	if err := s.googleJSON(req, &t, "token exchange"); err != nil {
		return nil, err
	}
	return &t, nil
}

func (s *Service) fetchUserinfo(ctx context.Context, cfg *GoogleOAuthConfig, accessToken string) (*googleUserInfo, error) {
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, cfg.userinfoURL(), nil)
	if err != nil {
		return nil, err
	}
	req.Header.Set("Authorization", "Bearer "+accessToken)
	var u googleUserInfo
	if err := s.googleJSON(req, &u, "userinfo"); err != nil {
		return nil, err
	}
	return &u, nil
}

// GoogleCallbackParams are the callback query parameters.
type GoogleCallbackParams struct{ Code, State, Error string }

// GoogleCallback completes a Google flow and returns where to redirect:
// the safe callback path (session started), /two-factor (2FA user, pending
// cookie set) or /login (Google reported an error). Checks, in order:
// browser binding, link-session binding, PKCE verifier — all before any
// network call.
func (s *Service) GoogleCallback(w http.ResponseWriter, r *http.Request, p GoogleCallbackParams) (string, error) {
	ctx := r.Context()
	if strings.TrimSpace(p.Error) != "" {
		slog.WarnContext(ctx, "Google OAuth denied or failed", "error", p.Error)
		return LoginPath, nil
	}
	code, state := strings.TrimSpace(p.Code), strings.TrimSpace(p.State)
	if code == "" {
		return "", BadRequest("code is required")
	}
	if state == "" {
		return "", BadRequest("state is required")
	}
	cfg, ok := s.GoogleConfig()
	if !ok {
		return "", BadRequest("Google sign-in is not configured (set GOOGLE_CLIENT_ID, GOOGLE_CLIENT_SECRET, and APP_ORIGIN)")
	}
	presented := s.takeBrowserBinding(w, r)
	st, err := s.consumeOAuthState(ctx, state)
	if err != nil {
		return "", err
	}
	if err := VerifyBrowserBinding(deref(st.BrowserBinding), presented); err != nil {
		return "", err
	}
	if st.LinkUserID != nil {
		var current *uuid.UUID
		if u, ok := CurrentUser(ctx); ok {
			current = &u.ID
		}
		if err := VerifyLinkSession(deref(st.SessionBinding), SessionTokenHash(r, s.secure()), *st.LinkUserID, current); err != nil {
			return "", err
		}
	}
	verifier := strings.TrimSpace(deref(st.CodeVerifier))
	if verifier == "" {
		return "", BadRequest("OAuth PKCE verifier missing; restart sign-in")
	}
	tokens, err := s.exchangeCode(ctx, cfg, code, verifier)
	if err != nil {
		slog.ErrorContext(ctx, "Google token exchange failed", "error", err)
		return "", BadRequest("Google token exchange failed")
	}
	profile, err := s.fetchUserinfo(ctx, cfg, tokens.AccessToken)
	if err != nil {
		slog.ErrorContext(ctx, "Google userinfo failed", "error", err)
		return "", BadRequest("Google userinfo failed")
	}
	email := NormalizeEmail(deref(profile.Email))
	if !strings.Contains(email, "@") {
		return "", BadRequest("Google account has no email")
	}
	name := strings.TrimSpace(deref(profile.Name))
	if name == "" {
		name, _, _ = strings.Cut(email, "@")
	}
	var image *string
	if pic := strings.TrimSpace(deref(profile.Picture)); pic != "" {
		image = &pic
	}
	acct := googleAccount{sub: profile.Sub, email: email, image: image, emailVerified: GoogleEmailVerified(profile.EmailVerified), scope: tokens.Scope}
	var userID uuid.UUID
	if st.LinkUserID != nil {
		userID, err = s.linkGoogleAccount(ctx, *st.LinkUserID, acct)
	} else {
		userID, err = s.findOrCreateGoogleUser(ctx, name, acct)
	}
	if err != nil {
		return "", err
	}
	ban, err := s.Q.AuthUserBanState(ctx, userID)
	if err != nil {
		return "", err
	}
	if IsCurrentlyBanned(ban.Banned, ban.BanExpires) {
		return "", ErrUnauthorized
	}
	// Google proves the first factor only: TOTP accounts go through the
	// pending-2FA step like a password sign-in (link flows are already in
	// an authenticated session).
	if st.LinkUserID == nil && ban.TwoFactorEnabled {
		token, err := s.CreatePending2FA(r, userID)
		if err != nil {
			return "", err
		}
		s.SetPending2FACookie(w, token)
		return "/two-factor", nil
	}
	if _, err := insertSession(ctx, w, r, s.Q, s.secure(), userID, nil); err != nil {
		return "", err
	}
	return st.Callback, nil
}

func deref(p *string) string {
	if p == nil {
		return ""
	}
	return *p
}

// googleAccount is the identity kept from Google: never its tokens.
type googleAccount struct {
	sub           string
	email         string
	image         *string
	emailVerified bool
	scope         *string
}

func (s *Service) refreshGoogleAccount(ctx context.Context, q *sqlc.Queries, a googleAccount) error {
	return q.AuthRefreshOAuthAccount(ctx, sqlc.AuthRefreshOAuthAccountParams{ProviderID: GoogleProviderID, AccountID: a.sub, Scope: a.scope})
}

func (s *Service) linkGoogleAccount(ctx context.Context, linkUserID uuid.UUID, a googleAccount) (uuid.UUID, error) {
	exists, err := s.Q.AuthUserExists(ctx, linkUserID)
	if err != nil {
		return uuid.Nil, err
	}
	if !exists {
		return uuid.Nil, BadRequest("user no longer exists")
	}
	owner, err := s.Q.AuthAccountOwner(ctx, sqlc.AuthAccountOwnerParams{ProviderID: GoogleProviderID, AccountID: a.sub})
	switch {
	case err == nil:
		if owner != linkUserID {
			return uuid.Nil, BadRequest("this Google account is already linked to another user")
		}
		if err := s.refreshGoogleAccount(ctx, s.Q, a); err != nil {
			return uuid.Nil, err
		}
	case errors.Is(err, pgx.ErrNoRows):
		if err := s.Q.AuthInsertAccount(ctx, sqlc.AuthInsertAccountParams{
			ID: newID(), AccountID: a.sub, ProviderID: GoogleProviderID, UserID: linkUserID, Scope: a.scope,
		}); err != nil {
			return uuid.Nil, err
		}
	default:
		return uuid.Nil, err
	}
	if a.emailVerified || a.image != nil {
		// A verified Google address proves only that address.
		if err := s.Q.AuthLinkGoogleProfile(ctx, sqlc.AuthLinkGoogleProfileParams{
			Verified: a.emailVerified, GoogleEmail: a.email, Image: a.image, ID: linkUserID,
		}); err != nil {
			return uuid.Nil, err
		}
	}
	return linkUserID, nil
}

func (s *Service) findOrCreateGoogleUser(ctx context.Context, name string, a googleAccount) (uuid.UUID, error) {
	owner, err := s.Q.AuthAccountOwner(ctx, sqlc.AuthAccountOwnerParams{ProviderID: GoogleProviderID, AccountID: a.sub})
	if err == nil {
		if err := s.refreshGoogleAccount(ctx, s.Q, a); err != nil {
			return uuid.Nil, err
		}
		if a.image != nil {
			if err := s.Q.AuthFillUserImage(ctx, sqlc.AuthFillUserImageParams{ID: owner, Image: a.image}); err != nil {
				return uuid.Nil, err
			}
		}
		return owner, nil
	}
	if !errors.Is(err, pgx.ErrNoRows) {
		return uuid.Nil, err
	}
	existing, err := s.Q.AuthUserLiteByEmail(ctx, a.email)
	found := err == nil
	if err != nil && !errors.Is(err, pgx.ErrNoRows) {
		return uuid.Nil, err
	}
	action, err := DecideGoogleEmailSignup(a.emailVerified, found)
	if err != nil {
		return uuid.Nil, err
	}
	var userID uuid.UUID
	switch action {
	case LinkExisting:
		if !existing.EmailVerified {
			// Pre-hijack guard: nobody proved control of this address;
			// Google just did, so hand the row over clean.
			if err := s.takeOverUnverifiedUser(ctx, existing.ID, name, a); err != nil {
				return uuid.Nil, err
			}
			return existing.ID, nil
		}
		if err := s.Q.AuthMarkGoogleVerified(ctx, sqlc.AuthMarkGoogleVerifiedParams{ID: existing.ID, Image: a.image}); err != nil {
			return uuid.Nil, err
		}
		userID = existing.ID
	default:
		userID = newID()
		if err := s.Q.AuthInsertUser(ctx, sqlc.AuthInsertUserParams{
			ID: userID, Name: name, Email: a.email, EmailVerified: a.emailVerified, Image: a.image, Role: strPtr("user"),
		}); err != nil {
			return uuid.Nil, err
		}
	}
	if err := s.Q.AuthInsertAccount(ctx, sqlc.AuthInsertAccountParams{
		ID: newID(), AccountID: a.sub, ProviderID: GoogleProviderID, UserID: userID, Scope: a.scope,
	}); err != nil {
		return uuid.Nil, err
	}
	return userID, nil
}

// takeOverUnverifiedUser drops every login method, session, second factor
// and user-scoped token of an unverified row, then links the verified
// Google owner. All-or-nothing.
func (s *Service) takeOverUnverifiedUser(ctx context.Context, userID uuid.UUID, name string, a googleAccount) error {
	err := db.WithTx(ctx, s.Pool, func(tx pgx.Tx) error {
		q := sqlc.New(tx)
		if err := q.AuthDeleteAccountsForUser(ctx, userID); err != nil {
			return err
		}
		if err := q.AuthDeleteSessionsForUser(ctx, userID); err != nil {
			return err
		}
		if err := q.AuthDeleteTwoFactor(ctx, userID); err != nil {
			return err
		}
		if err := q.AuthDeleteUserScopedVerifications(ctx, sqlc.AuthDeleteUserScopedVerificationsParams{
			Pending: Pending2FAPrefix + userID.String(), DeleteUser: DeleteIdentifierPrefix + userID.String(),
			ChangeEmailLike: ChangeEmailIdentifierPrefix + userID.String() + ":%",
		}); err != nil {
			return err
		}
		if err := q.AuthTakeOverUser(ctx, sqlc.AuthTakeOverUserParams{ID: userID, Name: name, Image: a.image}); err != nil {
			return err
		}
		return q.AuthInsertAccount(ctx, sqlc.AuthInsertAccountParams{
			ID: newID(), AccountID: a.sub, ProviderID: GoogleProviderID, UserID: userID, Scope: a.scope,
		})
	})
	if err != nil {
		return err
	}
	slog.WarnContext(ctx, "unverified account taken over by its verified Google owner; prior logins and sessions revoked", "user_id", userID)
	return nil
}
