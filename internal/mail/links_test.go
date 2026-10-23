package mail_test

import (
	"strings"
	"testing"

	"github.com/wellington-vell/lindaflor/internal/mail"
)

func TestUnsubscribeTokenIsSignedPerAddress(t *testing.T) {
	l := mail.Links{Pepper: "pepper"}
	token := l.UnsubscribeToken("Ana@Example.com")
	if len(token) != 64 {
		t.Fatal(token)
	}
	if !l.VerifyUnsubscribeToken(" ana@example.com ", token) {
		t.Fatal("case-insensitive, whitespace-tolerant")
	}
	if l.VerifyUnsubscribeToken("bia@example.com", token) {
		t.Fatal("other address")
	}
	if (mail.Links{Pepper: "other-pepper"}).VerifyUnsubscribeToken("ana@example.com", token) {
		t.Fatal("other pepper")
	}
	for _, bad := range []string{"", "zz", token[:62]} {
		if l.VerifyUnsubscribeToken("ana@example.com", bad) {
			t.Fatalf("accepted %q", bad)
		}
	}
	dev := mail.Links{}
	devToken := dev.UnsubscribeToken("ana@example.com")
	if !dev.VerifyUnsubscribeToken("ana@example.com", devToken) || devToken == token {
		t.Fatal("no pepper: still keyed")
	}
	// Domains separate purposes.
	if l.VerifyStockAlertCancelToken("ana@example.com", token) {
		t.Fatal("unsubscribe token must not cancel stock alerts")
	}
}

func TestUnsubscribeURLCarriesEmailAndValidToken(t *testing.T) {
	l := mail.Links{Pepper: "p"}
	url := l.UnsubscribeURL("Ana+news@Example.com")
	if !strings.HasPrefix(url, mail.DevOrigin+mail.UnsubscribePath+"?") || !strings.Contains(url, "email=ana%2Bnews%40example.com&token=") {
		t.Fatal(url)
	}
	token := url[strings.LastIndex(url, "token=")+len("token="):]
	if !l.VerifyUnsubscribeToken("ana+news@example.com", token) {
		t.Fatal("token must verify")
	}
	cancel := (mail.Links{Origin: "https://loja.example.com", Pepper: "p"}).StockAlertCancelURL("A@B.com")
	if !strings.HasPrefix(cancel, "https://loja.example.com/avise-me/cancelar?email=a%40b.com&token=") {
		t.Fatal(cancel)
	}
	ct := cancel[strings.LastIndex(cancel, "token=")+len("token="):]
	if !l.VerifyStockAlertCancelToken("a@b.com", ct) {
		t.Fatal("cancel token must verify")
	}
}
