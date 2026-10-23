package auth

import (
	"bytes"
	"encoding/base32"
	"errors"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"

	"github.com/google/uuid"
)

// --- validation.rs ---

// name_bounds
func TestNameBounds(t *testing.T) {
	for _, bad := range []string{"", "  ", strings.Repeat("a", 101)} {
		if _, err := ValidateName(bad); err == nil {
			t.Errorf("%q must be rejected", bad)
		}
	}
	if n, err := ValidateName("  Ana  "); err != nil || n != "Ana" {
		t.Fatalf("got %q %v", n, err)
	}
	if _, err := ValidateName(strings.Repeat("a", 100)); err != nil {
		t.Fatal(err)
	}
}

// email_rfc_ish
func TestEmailRFCish(t *testing.T) {
	for _, bad := range []string{"not-an-email", "a@b", "@x.com", strings.Repeat("a", 250) + "@ex.com", "a..b@x.com", "a b@x.com", ".a@x.com", "a@x.com.", "a@@x.com", "a!@x.com"} {
		if _, err := ValidateEmail(bad); err == nil {
			t.Errorf("%q must be rejected", bad)
		}
	}
	if _, err := ValidateEmail("a@b.c"); err != nil {
		t.Fatal(err)
	}
	if e, err := ValidateEmail("  Foo.Bar+tag@Example.COM "); err != nil || e != "foo.bar+tag@example.com" {
		t.Fatalf("got %q %v", e, err)
	}
}

// --- service.rs (pure) ---

// hash_prefix_is_eight_hex_and_not_raw
func TestHashPrefixIsEightHexAndNotRaw(t *testing.T) {
	raw := "super-secret-reset-token"
	hash := VerificationTokenHash(raw)
	prefix := VerificationTokenHashPrefix(hash)
	if len(prefix) != 8 || strings.Trim(prefix, "0123456789abcdef") != "" {
		t.Fatalf("bad prefix %q", prefix)
	}
	if hash == raw || strings.Contains(hash, raw) || strings.Contains("token_hash_prefix="+prefix, raw) {
		t.Fatal("hash leaks the raw secret")
	}
}

// peppered_hash_differs_from_plain_sha256
func TestPepperedHashDiffersFromPlainSHA256(t *testing.T) {
	plain := VerificationTokenHashWithPepper("token-with-pepper", "")
	mac := VerificationTokenHashWithPepper("token-with-pepper", "server-pepper")
	if plain == mac || len(plain) != 64 || len(mac) != 64 {
		t.Fatalf("plain=%s hmac=%s", plain, mac)
	}
}

func TestRandomTokenIsHex64(t *testing.T) {
	a, b := RandomToken(), RandomToken()
	if len(a) != 64 || a == b || strings.Trim(a, "0123456789abcdef") != "" {
		t.Fatalf("bad tokens %s %s", a, b)
	}
}

// --- user.rs ---

// ban_expiry_boundary
func TestBanExpiryBoundary(t *testing.T) {
	future, past := time.Now().Add(time.Hour), time.Now().Add(-time.Hour)
	if IsCurrentlyBanned(false, nil) || IsCurrentlyBanned(false, &future) {
		t.Fatal("never banned")
	}
	if !IsCurrentlyBanned(true, nil) || !IsCurrentlyBanned(true, &future) {
		t.Fatal("permanent / future bans hold")
	}
	if IsCurrentlyBanned(true, &past) {
		t.Fatal("expired temp ban must lift")
	}
}

// --- totp.rs ---

// generated_codes_keep_dddd_dddd_format
func TestGeneratedCodesKeepFormat(t *testing.T) {
	codes := GenerateBackupCodes()
	if len(codes) != 10 {
		t.Fatalf("got %d codes", len(codes))
	}
	for _, c := range codes {
		a, b, ok := strings.Cut(c, "-")
		if len(c) != 9 || !ok || len(a) != 4 || len(b) != 4 || strings.Trim(a+b, "0123456789") != "" {
			t.Fatalf("unexpected code %q", c)
		}
	}
}

// stored_entries_are_argon2id_phc
func TestStoredEntriesAreArgon2idPHC(t *testing.T) {
	stored, err := HashBackupCodes([]string{"ABCD-1234", "0000-0001"})
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(stored, "ABCD") {
		t.Fatal("plaintext leaked")
	}
	entries := strings.Split(stored, "\n")
	if len(entries) != 2 {
		t.Fatalf("got %d entries", len(entries))
	}
	for _, e := range entries {
		if !strings.HasPrefix(e, "$argon2id$v=19$m=8192,t=2,p=1$") {
			t.Fatalf("entry must be PHC: %s", e)
		}
		if _, err := parsePHC(e); err != nil {
			t.Fatal(err)
		}
	}
	if !strings.Contains(stored, ",") {
		t.Fatal("PHC params embed commas")
	}
}

// round_trip_consume_each_once
func TestRoundTripConsumeEachOnce(t *testing.T) {
	codes := GenerateBackupCodes()
	stored, err := HashBackupCodes(codes)
	if err != nil {
		t.Fatal(err)
	}
	for _, c := range codes {
		rest, ok := ConsumeBackupCode(stored, c)
		if !ok {
			t.Fatalf("code %s should match", c)
		}
		stored = rest
		if _, ok := ConsumeBackupCode(stored, c); ok {
			t.Fatalf("code %s reuse should fail", c)
		}
	}
	if stored != "" {
		t.Fatalf("all entries consumed, got %q", stored)
	}
}

// normalization_variants_match
func TestNormalizationVariantsMatch(t *testing.T) {
	stored, _ := HashBackupCodes([]string{"ABCD-1234"})
	for _, v := range []string{"abcd1234", "abcd-1234", "ABCD1234", "  abcd-1234  "} {
		if _, ok := ConsumeBackupCode(stored, v); !ok {
			t.Errorf("%q should match", v)
		}
	}
}

// wrong_code_returns_none
func TestWrongCodeReturnsNone(t *testing.T) {
	stored, _ := HashBackupCodes([]string{"ABCD-1234"})
	for _, v := range []string{"ZZZZ-9999", "", "---"} {
		if _, ok := ConsumeBackupCode(stored, v); ok {
			t.Errorf("%q must not match", v)
		}
	}
	if _, ok := ConsumeBackupCode(stored, "ABCD-1234"); !ok {
		t.Fatal("store must be unchanged")
	}
}

// tampered_phc_does_not_match
func TestTamperedPHCDoesNotMatch(t *testing.T) {
	stored, _ := HashBackupCodes([]string{"ABCD-1234"})
	last := stored[len(stored)-1]
	repl := byte('A')
	if last == 'A' {
		repl = 'B'
	}
	tampered := stored[:len(stored)-1] + string(repl)
	if _, ok := ConsumeBackupCode(tampered, "ABCD-1234"); ok {
		t.Fatal("tampered entry matched")
	}
	if _, ok := ConsumeBackupCode(stored, "ABCD-1234"); !ok {
		t.Fatal("untouched entry must verify")
	}
}

// malformed_entries_are_skipped_not_fatal
func TestMalformedEntriesAreSkippedNotFatal(t *testing.T) {
	stored, _ := HashBackupCodes([]string{"ABCD-1234"})
	for _, s := range []string{"not-valid\nno-dollar", "", "$argon2id$v=19$m=8192,t=2,p=1,broken"} {
		if _, ok := ConsumeBackupCode(s, "ABCD-1234"); ok {
			t.Fatalf("%q must not match", s)
		}
	}
	mixed := "garbage-no-dollar\n" + stored + "\n   \n$argon2id$broken"
	rest, ok := ConsumeBackupCode(mixed, "ABCD-1234")
	if !ok {
		t.Fatal("valid entry among garbage must match")
	}
	if rest != "garbage-no-dollar\n$argon2id$broken" {
		t.Fatalf("remaining = %q", rest)
	}
}

// non_phc_entries_never_verify
func TestNonPHCEntriesNeverVerify(t *testing.T) {
	stored, _ := HashBackupCodes([]string{"ABCD-1234"})
	if _, ok := ConsumeBackupCode("00112233445566778899aabbccddeeff$fc21e2dc2665f7e70258365faaac64fd04b9d327395956095b4bfdc4982a492d", "1234-5678"); ok {
		t.Fatal("hex$salt row matched")
	}
	if _, ok := ConsumeBackupCode(stored, "ZZZZ-9999"); ok {
		t.Fatal("wrong code matched")
	}
	if comma := strings.ReplaceAll(stored, "\n", ","); comma != stored {
		if _, ok := ConsumeBackupCode(comma, "ABCD-1234"); ok {
			t.Fatal("comma-joined store must not verify")
		}
	}
}

// empty_store_or_empty_code_returns_none
func TestEmptyStoreOrEmptyCodeReturnsNone(t *testing.T) {
	stored, _ := HashBackupCodes([]string{"ABCD-1234"})
	for _, c := range [][2]string{{"", "ABCD-1234"}, {"\n", "ABCD-1234"}, {stored, ""}, {stored, "   "}} {
		if _, ok := ConsumeBackupCode(c[0], c[1]); ok {
			t.Fatalf("%q/%q matched", c[0], c[1])
		}
	}
}

func TestTOTPGenerateAndVerify(t *testing.T) {
	secret, uri, err := GenerateTOTPSecret("ana@example.com")
	if err != nil {
		t.Fatal(err)
	}
	raw, err := base32.StdEncoding.WithPadding(base32.NoPadding).DecodeString(secret)
	if err != nil || len(raw) != 20 {
		t.Fatalf("secret must be 160-bit base32: %v", err)
	}
	if !strings.HasPrefix(uri, "otpauth://totp/lindaflor:ana@example.com?") || !strings.Contains(uri, "issuer=lindaflor") {
		t.Fatalf("uri %s", uri)
	}
	now := time.Now()
	for _, at := range []time.Time{now, now.Add(-30 * time.Second), now.Add(30 * time.Second)} {
		code, _ := TOTPCode(secret, at)
		if ok, err := verifyTOTPAt(secret, code, now); err != nil || !ok {
			t.Fatalf("skew=1 code rejected (%v)", err)
		}
	}
	far, _ := TOTPCode(secret, now.Add(2*time.Minute))
	if ok, _ := verifyTOTPAt(secret, far, now); ok {
		t.Fatal("code outside skew accepted")
	}
	for _, bad := range []string{"", "12345", "1234567", "abcdef"} {
		if ok, _ := verifyTOTPAt(secret, bad, now); ok {
			t.Fatalf("%q accepted", bad)
		}
	}
}

// --- totp_crypto.rs ---

var (
	keyA = bytes.Repeat([]byte{0x11}, 32)
	keyB = bytes.Repeat([]byte{0x22}, 32)
)

const secretB32 = "JBSWY3DPEHPK3PXPJBSWY3DPEHPK3PXP"

// round_trip_recovers_secret
func TestTOTPCryptoRoundTrip(t *testing.T) {
	enc, err := encryptWithKey(secretB32, keyA)
	if err != nil {
		t.Fatal(err)
	}
	if len(enc.Nonce) != TOTPNonceLen || enc.KeyVersion != TOTPSecretKeyVersion || bytes.Equal(enc.Ciphertext, []byte(secretB32)) {
		t.Fatalf("bad ciphertext %v", enc)
	}
	dec, err := decryptWithKey(enc.Nonce, enc.Ciphertext, enc.KeyVersion, keyA)
	if err != nil || dec != secretB32 {
		t.Fatalf("decrypt %q %v", dec, err)
	}
}

// nonces_are_random_per_encryption
func TestNoncesAreRandomPerEncryption(t *testing.T) {
	a, _ := encryptWithKey(secretB32, keyA)
	b, _ := encryptWithKey(secretB32, keyA)
	if bytes.Equal(a.Nonce, b.Nonce) || bytes.Equal(a.Ciphertext, b.Ciphertext) {
		t.Fatal("nonce reuse")
	}
}

// wrong_key_fails_closed
func TestWrongKeyFailsClosed(t *testing.T) {
	enc, _ := encryptWithKey(secretB32, keyA)
	if _, err := decryptWithKey(enc.Nonce, enc.Ciphertext, enc.KeyVersion, keyB); !errors.Is(err, ErrTOTPDecryptFailed) {
		t.Fatalf("got %v", err)
	}
}

// tampered_nonce_or_tag_fails_closed
func TestTamperedNonceOrTagFailsClosed(t *testing.T) {
	enc, _ := encryptWithKey(secretB32, keyA)
	nonce := append([]byte(nil), enc.Nonce...)
	nonce[0] ^= 1
	if _, err := decryptWithKey(nonce, enc.Ciphertext, enc.KeyVersion, keyA); !errors.Is(err, ErrTOTPDecryptFailed) {
		t.Fatalf("nonce: %v", err)
	}
	tag := append([]byte(nil), enc.Ciphertext...)
	tag[len(tag)-1] ^= 1
	if _, err := decryptWithKey(enc.Nonce, tag, enc.KeyVersion, keyA); !errors.Is(err, ErrTOTPDecryptFailed) {
		t.Fatalf("tag: %v", err)
	}
}

// short_nonce_is_rejected
func TestShortNonceIsRejected(t *testing.T) {
	enc, _ := encryptWithKey(secretB32, keyA)
	if _, err := decryptWithKey(enc.Nonce[:12], enc.Ciphertext, enc.KeyVersion, keyA); !errors.Is(err, ErrTOTPBadNonce) {
		t.Fatalf("got %v", err)
	}
}

// version_mismatch_is_rejected
func TestVersionMismatchIsRejected(t *testing.T) {
	enc, _ := encryptWithKey(secretB32, keyA)
	var v *UnsupportedKeyVersionError
	if _, err := decryptWithKey(enc.Nonce, enc.Ciphertext, TOTPSecretKeyVersion+1, keyA); !errors.As(err, &v) {
		t.Fatalf("got %v", err)
	}
}

// errors_and_debug_carry_no_secret_material
func TestErrorsAndDebugCarryNoSecretMaterial(t *testing.T) {
	enc, _ := encryptWithKey(secretB32, keyA)
	for _, s := range []string{
		enc.String(), enc.GoString(), ErrTOTPDecryptFailed.Error(), ErrTOTPBadNonce.Error(),
		ErrTOTPInvalidUTF8.Error(), (&UnsupportedKeyVersionError{Version: 9}).Error(),
	} {
		if strings.Contains(s, secretB32) {
			t.Fatalf("leak: %s", s)
		}
	}
	_, err := decryptWithKey(enc.Nonce, enc.Ciphertext, enc.KeyVersion, keyB)
	if strings.Contains(err.Error(), secretB32) {
		t.Fatal("failed decrypt leaked plaintext")
	}
}

// missing_or_tampered_secret_fails_hard (tests/auth_two_factor.rs)
func TestMissingOrTamperedSecretFailsHard(t *testing.T) {
	t.Setenv("TOTP_ENCRYPTION_KEY", "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=")
	if _, err := DecryptTOTPSecret(make([]byte, 24), []byte("tag"), 1); err == nil {
		t.Fatal("random nonce/ciphertext must fail")
	}
	if _, err := DecryptTOTPSecret(make([]byte, 24), []byte("tag"), 999); err == nil {
		t.Fatal("version mismatch must fail")
	}
	if _, err := DecryptTOTPSecret(make([]byte, 12), []byte("tag"), 1); err == nil {
		t.Fatal("short nonce must fail")
	}
}

// --- google.rs / oauth.rs ---

func testGoogle() *GoogleOAuthConfig {
	return &GoogleOAuthConfig{ClientID: "fake-client.apps.googleusercontent.com", ClientSecret: "GOCSPX-fake", AppOrigin: "http://localhost:4200"}
}

// authorize_url_includes_callback_and_state
func TestAuthorizeURLIncludesCallbackAndState(t *testing.T) {
	u := testGoogle().AuthorizeURL("csrf-state-1", "")
	for _, want := range []string{
		"https://accounts.google.com/o/oauth2/v2/auth?", "client_id=fake-client.apps.googleusercontent.com",
		"state=csrf-state-1", "redirect_uri=http%3A%2F%2Flocalhost%3A4200%2Fapi%2Fauth%2Fcallback%2Fgoogle",
		"scope=openid+email+profile",
	} {
		if !strings.Contains(u, want) {
			t.Errorf("%s missing %s", u, want)
		}
	}
	if strings.Contains(u, "code_challenge") || strings.Contains(u, "nonce=") {
		t.Fatal(u)
	}
}

// authorize_url_with_pkce_has_no_nonce / authorize_url_carries_pkce_without_nonce
func TestAuthorizeURLWithPKCEHasNoNonce(t *testing.T) {
	u := testGoogle().AuthorizeURL("csrf-state-1", "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM")
	for _, want := range []string{"code_challenge=E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM", "code_challenge_method=S256", "state=csrf-state-1"} {
		if !strings.Contains(u, want) {
			t.Errorf("missing %s", want)
		}
	}
	if strings.Contains(u, "nonce=") {
		t.Fatal(u)
	}
	if cfg := (&GoogleOAuthConfig{TestBase: "http://stub"}); cfg.tokenURL() != "http://stub/token" || cfg.userinfoURL() != "http://stub/userinfo" {
		t.Fatal("test base not honored")
	}
}

// safe_callback_rejects_open_redirect
func TestSafeCallbackRejectsOpenRedirect(t *testing.T) {
	cases := map[string]string{
		"/conta": "/conta", "https://evil.test": "/dashboard", "//evil.test": "/dashboard",
		"": "/dashboard", "/\\evil.test": "/dashboard", "/\t/evil.test": "/dashboard",
	}
	for in, want := range cases {
		if got := SafeCallbackPath(in); got != want {
			t.Errorf("%q → %q, want %q", in, got, want)
		}
	}
}

// missing_email_verified_claim_is_not_verified
func TestMissingEmailVerifiedClaimIsNotVerified(t *testing.T) {
	f, tr := false, true
	if GoogleEmailVerified(nil) || GoogleEmailVerified(&f) || !GoogleEmailVerified(&tr) {
		t.Fatal("claim handling")
	}
}

// unverified_claim_does_not_link_or_create
func TestUnverifiedClaimDoesNotLinkOrCreate(t *testing.T) {
	for _, existing := range []bool{true, false} {
		if _, err := DecideGoogleEmailSignup(GoogleEmailVerified(nil), existing); err == nil || !strings.Contains(err.Error(), "verify your Google email") {
			t.Fatalf("existing=%v: %v", existing, err)
		}
	}
}

// verified_claim_links_or_creates
func TestVerifiedClaimLinksOrCreates(t *testing.T) {
	if a, err := DecideGoogleEmailSignup(true, true); err != nil || a != LinkExisting {
		t.Fatal(a, err)
	}
	if a, err := DecideGoogleEmailSignup(true, false); err != nil || a != CreateNew {
		t.Fatal(a, err)
	}
}

// pkce_challenge_matches_rfc7636_vector
func TestPKCEChallengeMatchesRFC7636Vector(t *testing.T) {
	if got := PKCES256Challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"); got != "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM" {
		t.Fatal(got)
	}
	for _, v := range []string{"a", "ab", "abc", "abcd"} {
		if c := PKCES256Challenge(v); strings.ContainsAny(c, "+/=") {
			t.Fatal(c)
		}
	}
}

// link_session_binding_accepts_only_matching_session_and_user
func TestLinkSessionBindingAcceptsOnlyMatchingSessionAndUser(t *testing.T) {
	link, other := uuid.New(), uuid.New()
	if err := VerifyLinkSession("hash-a", "hash-a", link, &link); err != nil {
		t.Fatal(err)
	}
	bad := []struct {
		exp, act string
		user     *uuid.UUID
	}{
		{"hash-a", "hash-b", &link},
		{"", "hash-a", &link},
		{"hash-a", "", &link},
		{"hash-a", "hash-a", &other},
		{"hash-a", "hash-a", nil},
	}
	for _, b := range bad {
		if err := VerifyLinkSession(b.exp, b.act, link, b.user); err == nil {
			t.Fatalf("%+v accepted", b)
		}
	}
}

func TestVerifyBrowserBinding(t *testing.T) {
	if VerifyBrowserBinding("a", "a") != nil || VerifyBrowserBinding("", "a") == nil || VerifyBrowserBinding("a", "") == nil || VerifyBrowserBinding("a", "b") == nil {
		t.Fatal("binding rules")
	}
}

// --- session cookies ---

func TestSessionCookieAttributes(t *testing.T) {
	for _, secure := range []bool{false, true} {
		rec := httptest.NewRecorder()
		setSessionCookie(rec, "raw", secure)
		ClearCookie(rec, "other", secure)
		lines := rec.Header().Values("Set-Cookie")
		if len(lines) != 2 {
			t.Fatalf("lines %v", lines)
		}
		set := lines[0]
		if !strings.HasPrefix(set, SessionCookieName(secure)+"=raw") || !strings.Contains(set, "HttpOnly") ||
			!strings.Contains(set, "SameSite=Lax") || !strings.Contains(set, "Path=/") ||
			strings.Contains(set, "Secure") != secure || !strings.Contains(set, "Max-Age=2592000") {
			t.Fatalf("secure=%v: %s", secure, set)
		}
		if !strings.Contains(lines[1], "Max-Age=0") {
			t.Fatalf("clear: %s", lines[1])
		}
	}
	// A later write for the same name replaces the earlier line.
	rec := httptest.NewRecorder()
	ClearCookie(rec, SessionCookieDev, false)
	setSessionCookie(rec, "new", false)
	if lines := rec.Header().Values("Set-Cookie"); len(lines) != 1 || !strings.HasPrefix(lines[0], "session=new") {
		t.Fatalf("lines %v", lines)
	}
}

func TestRequireGuards(t *testing.T) {
	ok := http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) { w.WriteHeader(http.StatusTeapot) })
	req := httptest.NewRequest(http.MethodGet, "/conta/pedidos?x=1", nil)
	rec := httptest.NewRecorder()
	RequireUser(ok).ServeHTTP(rec, req)
	if rec.Code != http.StatusSeeOther || rec.Header().Get("Location") != "/login?next=%2Fconta%2Fpedidos%3Fx%3D1" {
		t.Fatalf("anon: %d %s", rec.Code, rec.Header().Get("Location"))
	}
	role := "user"
	u := &User{ID: uuid.New(), Role: &role}
	req = req.WithContext(WithUser(req.Context(), u, &Session{}))
	rec = httptest.NewRecorder()
	RequireAdmin(ok).ServeHTTP(rec, req)
	if rec.Code != http.StatusSeeOther || rec.Header().Get("Location") != "/dashboard" {
		t.Fatalf("non-admin: %d %s", rec.Code, rec.Header().Get("Location"))
	}
	admin := "Admin"
	u.Role = &admin
	rec = httptest.NewRecorder()
	RequireAdmin(ok).ServeHTTP(rec, req)
	if rec.Code != http.StatusTeapot {
		t.Fatalf("admin: %d", rec.Code)
	}
}
