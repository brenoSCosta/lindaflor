package toast

import (
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

func TestLegacyPrefixesStillDecode(t *testing.T) {
	ok := DecodeQueue("ok:Conta criada.")
	if len(ok) != 1 || ok[0].Type != SuccessType || ok[0].Title != "Conta criada." {
		t.Fatalf("ok: %+v", ok)
	}
	er := DecodeQueue("err:Senha incorreta")
	if er[0].Type != ErrorType || er[0].Title != "Senha incorreta" {
		t.Fatalf("err: %+v", er)
	}
	if len(DecodeQueue("err:   ")) != 0 {
		t.Fatal("blank legacy toast must be dropped")
	}
}

func TestJSONQueueRoundTripsThroughTheCookieEncoding(t *testing.T) {
	encoded := encodeToastValue([]Toast{Success("Olá").WithDescription("Tudo bem").WithAction("Ver", "/conta")})
	if strings.ContainsAny(encoded, "\"; ,\\") {
		t.Fatalf("encoding must be a valid cookie value: %s", encoded)
	}
	d := DecodeQueue(encoded)
	if d[0].Title != "Olá" || d[0].Description != "Tudo bem" || d[0].Action.Href != "/conta" || d[0].Type != SuccessType || d[0].Shown || !d[0].Dismissible {
		t.Fatalf("roundtrip: %+v", d[0])
	}
	seen := Success("Já estava")
	seen.Shown = true
	raw, _ := json.Marshal([]Toast{seen})
	if !DecodeQueue(string(raw))[0].Shown {
		t.Fatal("shown flag lost")
	}
}

func TestQueueKeepsTheNewestFive(t *testing.T) {
	var q []Toast
	for i := range 7 {
		q = enqueueToast(q, Info(fmt.Sprintf("n%d", i)), false)
	}
	if len(q) != 5 || q[0].Title != "n2" || q[4].Title != "n6" {
		t.Fatalf("%+v", q)
	}
}

func TestQueueRespectsCookieBudget(t *testing.T) {
	var q []Toast
	for i := range 5 {
		q = enqueueToast(q, Info(strings.Repeat("é", 180)).WithDescription(fmt.Sprintf("%d%s", i, strings.Repeat("ç", 170))), true)
	}
	if n := len(toastCookie(encodeToastValue(q), true).String()); n > cookieBudget || len(q) == 0 {
		t.Fatalf("over budget: %d bytes, %d toasts", n, len(q))
	}
}

func TestSanitizeClipsAndDropsEmpty(t *testing.T) {
	if _, ok := sanitizeToast(Plain("   ")); ok {
		t.Fatal("empty title must be dropped")
	}
	tt, _ := sanitizeToast(Plain(strings.Repeat("a", 500)).WithAction(" ", "/x"))
	if len(tt.Title) != toastMaxChars || tt.Action != nil {
		t.Fatalf("%+v", tt)
	}
}

func TestSetThenTakeFullPage(t *testing.T) {
	h := Middleware(true)(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		Set(w, r, Success("Salvo"))
		Set(w, r, Error("Falhou"))
		Redirect(w, r, "/conta")
	}))
	rec := httptest.NewRecorder()
	h.ServeHTTP(rec, httptest.NewRequest(http.MethodPost, "/x", nil))
	if rec.Code != http.StatusSeeOther || rec.Header().Get("Location") != "/conta" {
		t.Fatalf("redirect: %d %v", rec.Code, rec.Header())
	}
	cookies := rec.Result().Cookies()
	if len(cookies) != 1 || cookies[0].Name != CookieSecure || !cookies[0].Secure || !cookies[0].HttpOnly || cookies[0].Path != "/" {
		t.Fatalf("cookies: %+v", cookies)
	}
	// Next request renders and clears them.
	req := httptest.NewRequest(http.MethodGet, "/conta", nil)
	req.AddCookie(cookies[0])
	rec2 := httptest.NewRecorder()
	var got []Toast
	Middleware(true)(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) { got = Take(w, r) })).ServeHTTP(rec2, req)
	if len(got) != 2 || got[0].Title != "Salvo" || got[1].Title != "Falhou" {
		t.Fatalf("take: %+v", got)
	}
	cl := rec2.Result().Cookies()
	if len(cl) != 1 || cl[0].MaxAge >= 0 || !cl[0].Secure || !cl[0].HttpOnly {
		t.Fatalf("clear cookie: %+v", cl)
	}
}

func TestSetOnHTMXUsesHeader(t *testing.T) {
	h := Middleware(false)(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		Set(w, r, Success("Adicionado ao carrinho"))
		Redirect(w, r, "/carrinho")
	}))
	req := httptest.NewRequest(http.MethodPost, "/carrinho/add", nil)
	req.Header.Set("HX-Request", "true")
	rec := httptest.NewRecorder()
	h.ServeHTTP(rec, req)
	if rec.Code != http.StatusOK || rec.Header().Get("HX-Location") != "/carrinho" {
		t.Fatalf("htmx redirect: %d %v", rec.Code, rec.Header())
	}
	if len(rec.Result().Cookies()) != 0 {
		t.Fatal("htmx toasts must not use the cookie")
	}
	q := DecodeQueue(rec.Header().Get(Header))
	if len(q) != 1 || q[0].Title != "Adicionado ao carrinho" {
		t.Fatalf("header: %q", rec.Header().Get(Header))
	}
}
