package store_test

import (
	"strings"
	"testing"

	"github.com/google/uuid"

	"github.com/wellington-vell/lindaflor/internal/store"
)

// Pure-function ports: pricing.rs, shipping.rs, order_timeline.rs,
// queries.rs (whatsapp), reviews.rs (parsing), pedido/id.rs (access),
// catalog filters, utils.rs (image classify), admin validators.

func TestPixPrice(t *testing.T) {
	if store.PixPrice(10000, 5) != 9500 || store.PixPrice(10000, 10) != 9000 || store.PixPrice(10001, 5) != 9501 {
		t.Fatal("pix price")
	}
}

func TestPixDiscountCents(t *testing.T) {
	for _, c := range []struct {
		cents int64
		pct   int
		want  int64
	}{{10000, 5, 500}, {10000, 10, 1000}, {10001, 5, 500}, {-100, 5, 0}, {10000, 0, 0}} {
		if got := store.PixDiscountCents(c.cents, c.pct); got != c.want {
			t.Errorf("%+v: %d", c, got)
		}
	}
}

func TestDiscountPercent(t *testing.T) {
	if p, ok := store.DiscountPercent(price(10000), 9000); !ok || p != 10 {
		t.Fatal(p)
	}
	if _, ok := store.DiscountPercent(price(10000), 10000); ok {
		t.Fatal("equal")
	}
	if _, ok := store.DiscountPercent(nil, 5000); ok {
		t.Fatal("nil")
	}
}

func TestCEPBoundariesMapToUF(t *testing.T) {
	for cep, uf := range map[string]string{
		"01000000": "SP", "19999999": "SP", "20000000": "RJ", "28999999": "RJ", "29000000": "ES",
		"48999999": "BA", "49000000": "SE", "49099999": "SE", "49999999": "SE", "50000000": "PE",
		"57000000": "AL", "65999999": "MA", "66000000": "PA", "68899999": "PA", "68900000": "AP",
		"69000000": "AM", "69300000": "RR", "69399999": "RR", "69400000": "AM", "69900000": "AC",
		"70000000": "DF", "72800000": "GO", "73000000": "DF", "73700000": "GO", "76799999": "GO",
		"76800000": "RO", "77000000": "TO", "78000000": "MT", "78899999": "MT", "79000000": "MS",
		"79999999": "MS", "80000000": "PR", "88000000": "SC", "90000000": "RS", "99999999": "RS",
	} {
		if got, ok := store.UFFromCEP(cep); !ok || got != uf {
			t.Errorf("CEP %s → %s", cep, got)
		}
	}
}

func TestEveryStateHasACEPRange(t *testing.T) {
	seen := map[string]bool{}
	for p := 1000; p <= 99999; p++ {
		if uf, ok := store.UFFromCEP(strings.Repeat("0", 5-len(itoa(p))) + itoa(p) + "000"); ok {
			seen[uf] = true
			if !store.IsBrazilianState(uf) {
				t.Fatalf("%s not a UF", uf)
			}
		}
	}
	for _, uf := range store.BrazilianStates {
		if !seen[uf] {
			t.Errorf("%s missing", uf)
		}
	}
}

func itoa(n int) string {
	s := ""
	for n > 0 {
		s = string(rune('0'+n%10)) + s
		n /= 10
	}
	return s
}

func TestInvalidCEPsAreRejected(t *testing.T) {
	for _, cep := range []string{"", "4900000", "490000000", "4900000a", "00000000", "00999999", "49000-00"} {
		if _, ok := store.UFFromCEP(cep); ok {
			t.Errorf("%q accepted", cep)
		}
	}
}

func TestResolveAcceptsFormattedMatchingCEP(t *testing.T) {
	if uf, err := store.ResolveShippingUF("49000-000", "SE"); err != nil || uf != "SE" {
		t.Fatal(uf, err)
	}
	if uf, err := store.ResolveShippingUF(" 01310-100 ", " SP "); err != nil || uf != "SP" {
		t.Fatal(uf, err)
	}
}

func TestResolveRejectsBadInput(t *testing.T) {
	if _, err := store.ResolveShippingUF("", "SE"); userMsg(t, err) != "Informe o CEP." {
		t.Fatal(err)
	}
	for _, c := range [][2]string{{"4900-000", "SE"}, {"00000-000", "SE"}, {"49000-000", "XX"}, {"49000-000", ""}} {
		if _, err := store.ResolveShippingUF(c[0], c[1]); err == nil {
			t.Errorf("%v accepted", c)
		}
	}
}

func TestMismatchedStateCannotGetCheaperRate(t *testing.T) {
	_, err := store.ResolveShippingUF("66010-000", "SE")
	if !strings.Contains(userMsg(t, err), "PA") {
		t.Fatal(err)
	}
	if _, err := store.ResolveShippingUF("01310-100", "SE"); err == nil {
		t.Fatal("SP as SE")
	}
	uf, _ := store.UFFromCEP("66010000")
	if c, free := store.CalculateShipping(10000, uf, 29900); c != 5990 || free {
		t.Fatal(c)
	}
}

func TestTiersFollowRegion(t *testing.T) {
	if store.TierForUF("SE") != store.TierLocal {
		t.Fatal("SE")
	}
	for _, uf := range []string{"AC", "AM", "AP", "PA", "RO", "RR", "TO"} {
		if store.TierForUF(uf) != store.TierRemote {
			t.Error(uf)
		}
	}
	for _, uf := range []string{"MT", "MS", "GO", "DF", "SP", "RJ", "RS", "BA", "AL", "PE"} {
		if store.TierForUF(uf) != store.TierStandard {
			t.Error(uf)
		}
	}
	for uf, want := range map[string]int64{"SE": 1990, "MT": 3990, "AM": 5990} {
		if c, _ := store.CalculateShipping(10000, uf, 29900); c != want {
			t.Errorf("%s %d", uf, c)
		}
	}
}

func TestCalculateShippingHonorsThreshold(t *testing.T) {
	for _, c := range []struct {
		sub       int64
		uf        string
		threshold int64
	}{{29900, "SE", 29900}, {1, "AM", 0}, {5000, "SP", 4999}} {
		if cents, free := store.CalculateShipping(c.sub, c.uf, c.threshold); cents != 0 || !free {
			t.Errorf("%+v", c)
		}
	}
}

func TestStepsFollowStatus(t *testing.T) {
	for s, want := range map[string]int{"pending_payment": 0, "processing": 1, "shipped": 2, "delivered": 3} {
		if got, ok := store.ReachedStep(s); !ok || got != want {
			t.Errorf("%s %d", s, got)
		}
	}
	if _, ok := store.ReachedStep("cancelled"); ok {
		t.Fatal("cancelled")
	}
	p := store.OrderProgress{Status: "shipped", CreatedAt: str("01/01/2026 10:00"), PaidAt: str("01/01/2026 11:00"), Carrier: "Correios", TrackingCode: str("AA123456789BR")}
	steps := p.Steps()
	if len(steps) != 4 || !steps[2].Done || steps[3].Done || steps[1].Date != "01/01/2026 11:00" || steps[2].Date != "" {
		t.Fatalf("%+v", steps)
	}
	if !strings.Contains(p.TrackingLink(), "AA123456789BR") {
		t.Fatal(p.TrackingLink())
	}
}

func TestRenderWhatsappTemplateReplacesKnownVars(t *testing.T) {
	got := store.RenderWhatsappTemplate(str("Pedido {{order_id}} — {{total}}"), [][2]string{{"order_id", "abc123"}, {"total", "R$ 10,00"}}, "fallback")
	if got != "Pedido abc123 — R$ 10,00" {
		t.Fatal(got)
	}
}

func TestRenderWhatsappTemplateFallsBackOnLeftoverPlaceholders(t *testing.T) {
	got := store.RenderWhatsappTemplate(str("Fiz o pedido {{order_id}} no valor de {{total}}"), [][2]string{{"product", "Maiô"}}, "Olá! Tenho interesse no Maiô.")
	if got != "Olá! Tenho interesse no Maiô." {
		t.Fatal(got)
	}
	if got := store.WhatsappLink("5579999816511", "Olá! Pedido #1"); got != "https://wa.me/5579999816511?text=Ol%C3%A1%21+Pedido+%231" {
		t.Fatal(got)
	}
}

func TestReviewParseValidatesFields(t *testing.T) {
	ok, err := store.ParseNewReview("5", "  Ótimo ", " Amei ", str("ideal"))
	if err != nil || ok.Rating != 5 || ok.Title != "Ótimo" || ok.Body != "Amei" || ok.Fit == nil || *ok.Fit != store.FitIdeal {
		t.Fatal(ok, err)
	}
	if r, _ := store.ParseNewReview("5", "t", "b", str("")); r.Fit != nil {
		t.Fatal("blank fit")
	}
	for _, rating := range []string{"0", "6", "", "x", "4.5"} {
		if _, err := store.ParseNewReview(rating, "t", "b", nil); err == nil {
			t.Error(rating)
		}
	}
	for _, c := range [][3]string{{"3", " ", "b"}, {"3", strings.Repeat("a", 81), "b"}, {"3", "t", strings.Repeat("b", 2001)}} {
		if _, err := store.ParseNewReview(c[0], c[1], c[2], nil); err == nil {
			t.Error(c)
		}
	}
	if _, err := store.ParseNewReview("3", strings.Repeat("á", 80), "b", nil); err != nil {
		t.Fatal(err)
	}
	if _, err := store.ParseNewReview("3", "t", "b", str("enorme")); err == nil {
		t.Fatal("bad fit")
	}
}

func TestPublicAuthorKeepsFirstNameAndInitial(t *testing.T) {
	for in, want := range map[string]string{"Maria da Silva": "Maria S.", "ana": "ana", "  ": "Cliente"} {
		if got := store.PublicAuthor(in); got != want {
			t.Errorf("%q → %q", in, got)
		}
	}
}

func TestFitMajorityReportsShare(t *testing.T) {
	label, pct, ok := store.RatingSummary{FitPequeno: 1, FitIdeal: 3}.FitMajority()
	if !ok || label != "Tamanho ideal" || pct != 75 {
		t.Fatal(label, pct)
	}
	if _, _, ok := (store.RatingSummary{}).FitMajority(); ok {
		t.Fatal("empty")
	}
	if store.FormatAverage(4.45) != "4,5" && store.FormatAverage(4.45) != "4,4" {
		t.Fatal(store.FormatAverage(4.45))
	}
	if store.ReviewsCountLabel(1) != "1 avaliação" || store.ReviewsCountLabel(3) != "3 avaliações" {
		t.Fatal("count label")
	}
}

func TestTokenOrOwnerAllows(t *testing.T) {
	token := uuid.New()
	owner := newID()
	tok := token.String()
	wrong := uuid.NewString()
	if !store.TokenOrOwnerAllows(token, false, nil, &tok, nil) {
		t.Fatal("matching token")
	}
	if store.TokenOrOwnerAllows(token, false, nil, nil, nil) || store.TokenOrOwnerAllows(token, false, nil, &wrong, nil) {
		t.Fatal("wrong/missing token")
	}
	other := newID()
	if !store.TokenOrOwnerAllows(token, false, &owner, nil, &owner) || store.TokenOrOwnerAllows(token, false, &owner, nil, &other) {
		t.Fatal("owner session")
	}
	if store.TokenOrOwnerAllows(token, true, nil, &tok, nil) || !store.TokenOrOwnerAllows(token, true, &owner, nil, &owner) {
		t.Fatal("expired token")
	}
}

func TestPriceParamAcceptsCommaAndRejectsJunk(t *testing.T) {
	for raw, want := range map[string]int64{"89,90": 8990, "100": 10000} {
		if got := store.PriceParamCents(str(raw)); got == nil || *got != want {
			t.Errorf("%s", raw)
		}
	}
	for _, raw := range []string{"-1", "NaN", "abc"} {
		if store.PriceParamCents(str(raw)) != nil {
			t.Errorf("%s accepted", raw)
		}
	}
	if store.PriceParamCents(nil) != nil {
		t.Fatal("nil")
	}
}

func TestCatalogFiltersDropUnknownSizesAndCapColors(t *testing.T) {
	var colors []string
	for i := range 30 {
		colors = append(colors, "cor"+itoa(i+1))
	}
	f := store.NormalizeCatalogFilters([]string{"P", "xxl", "m"}, colors, nil, nil, false, true)
	if strings.Join(f.Sizes, ",") != "p,m" || len(f.Colors) != 20 || !f.SaleOnly {
		t.Fatalf("%+v", f)
	}
}

func TestCatalogParamsParseRawQuery(t *testing.T) {
	p := store.ParseCatalogParams("category=maio&search=+azul+&destaque=true&page=3&sort=novos&tamanho=p&tamanho=m&cor=Azul&preco_min=89,90&disponivel=true")
	if p.Category != "maio" || p.Search != "azul" || !p.FeaturedOnly || p.IncludeOutOfStock || p.Page != 3 || p.Sort != "novos" ||
		strings.Join(p.Sizes, ",") != "p,m" || strings.Join(p.Colors, ",") != "Azul" || p.PrecoMin == nil || *p.PrecoMin != "89,90" ||
		p.PrecoMax != nil || !p.Disponivel || p.Sale {
		t.Fatalf("%+v", p)
	}
	j := store.ParseCatalogParams("category=nope&page=abc&destaque=yes")
	if j.Category != "all" || j.Page != 1 || j.FeaturedOnly || j.Sort != "destaque" {
		t.Fatalf("%+v", j)
	}
}

func TestRepeatedValuesAreCollectedAndDecoded(t *testing.T) {
	pairs := store.QueryPairs("tamanho=p&cor=Azul+marinho&tamanho=m&tamanho=&cor=Lim%C3%A3o")
	if strings.Join(store.RepeatedValues(pairs, "tamanho"), ",") != "p,m" || strings.Join(store.RepeatedValues(pairs, "cor"), ",") != "Azul marinho,Limão" {
		t.Fatal(pairs)
	}
	if len(store.RepeatedValues(store.QueryPairs(""), "cor")) != 0 {
		t.Fatal("empty")
	}
}

func filterState() store.CatalogParams {
	return store.CatalogParams{
		Category: "all", Search: "flor & mar", Sort: "novos", Sizes: []string{"p", "m"},
		Colors: []string{"Azul-marinho #2"}, PrecoMin: str("1&sale=true"), Disponivel: true,
	}
}

func TestHrefEncodesValuesAndKeepsBasePath(t *testing.T) {
	var saleHref string
	p := filterState()
	p.Sale = true
	for _, c := range p.Chips("/sets/verao") {
		if c.Label == "Em promoção" {
			saleHref = c.Href
		}
	}
	for _, want := range []string{"search=flor+%26+mar", "cor=Azul-marinho+%232", "preco_min=1%26sale%3Dtrue"} {
		if !strings.Contains(saleHref, want) {
			t.Errorf("%s lacks %s", saleHref, want)
		}
	}
	if !strings.HasPrefix(saleHref, "/sets/verao?") || strings.Contains(saleHref, "&sale=true") {
		t.Fatal(saleHref)
	}
}

func TestHrefDropsOnlyTheRequestedFilter(t *testing.T) {
	p := filterState()
	chips := p.Chips("/sets/verao")
	if chips[0].Label != "P" || !strings.Contains(chips[0].Href, "tamanho=m") || strings.Contains(chips[0].Href, "tamanho=p&") {
		t.Fatalf("%+v", chips[0])
	}
	if got := p.ClearFacetsHref("/sets/verao"); got != "/sets/verao?search=flor+%26+mar&sort=novos" {
		t.Fatal(got)
	}
}

func TestImageClassification(t *testing.T) {
	png := append([]byte{0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A}, 0, 0, 0, 0)
	webp := append(append([]byte("RIFF"), 0, 0, 0, 0), []byte("WEBPxxxx")...)
	if k, ok := store.ClassifyImage("image/jpeg", []byte{0xFF, 0xD8, 0xFF, 0xD9}); !ok || k.Extension != "jpg" {
		t.Fatal("jpeg")
	}
	if _, ok := store.ClassifyImage("image/png; charset=binary", png); !ok {
		t.Fatal("png")
	}
	if _, ok := store.ClassifyImage("image/webp", webp); !ok {
		t.Fatal("webp")
	}
	if _, ok := store.ClassifyImage("image/png", []byte{0xFF, 0xD8, 0xFF}); ok {
		t.Fatal("mismatch")
	}
	if _, err := store.ValidateImage(nil, "image/png"); userMsg(t, err) != store.MsgImageType {
		t.Fatal(err)
	}
	big := make([]byte, store.MaxImageBytes+1)
	copy(big, png)
	if _, err := store.ValidateImage(big, "image/png"); userMsg(t, err) != store.MsgImageSize {
		t.Fatal(err)
	}
}

func TestBannerLinkMustBeInternalPath(t *testing.T) {
	if l, err := store.ValidateBannerLink(""); err != nil || l != nil {
		t.Fatal(l, err)
	}
	if l, err := store.ValidateBannerLink(" /produtos?sale=1 "); err != nil || *l != "/produtos?sale=1" {
		t.Fatal(l, err)
	}
	for _, bad := range []string{"https://evil.example", "//evil.example", "/\\evil.example", "/\t/evil.example"} {
		if _, err := store.ValidateBannerLink(bad); err == nil {
			t.Error(bad)
		}
	}
}

func TestManualOverrideRules(t *testing.T) {
	for _, ev := range []store.OrderEvent{store.EventPay, store.EventReopenPay, store.EventRefund} {
		if !store.ManualMoneyEventBlocked(true, ev) || store.ManualMoneyEventBlocked(false, ev) {
			t.Error(ev)
		}
	}
	for _, ev := range []store.OrderEvent{store.EventProcess, store.EventShip, store.EventDeliver, store.EventCancel, store.EventReopen} {
		if store.ManualMoneyEventBlocked(true, ev) {
			t.Error(ev)
		}
	}
	orderID := newID()
	code := store.OrderCode(orderID)
	reason := "MP fora do ar, PIX conferido no extrato"
	if r, err := store.CheckManualOverride(orderID, strings.ToLower(code), reason); err != nil || r != reason {
		t.Fatal(r, err)
	}
	if _, err := store.CheckManualOverride(orderID, " #"+code+" ", reason); err != nil {
		t.Fatal(err)
	}
	for _, c := range [][2]string{{"", reason}, {"DEADBEEF", reason}, {code, "curto"}, {code, strings.Repeat("x", 501)}} {
		if _, err := store.CheckManualOverride(orderID, c[0], c[1]); err == nil {
			t.Error(c)
		}
	}
	if s, _, _, _ := store.ManualOverrideAction("pending_payment", false); s != "paid" {
		t.Fatal(s)
	}
	for _, st := range []string{"paid", "processing", "shipped", "delivered"} {
		if s, _, restock, _ := store.ManualOverrideAction(st, false); s != "refunded" || !restock {
			t.Fatal(st)
		}
	}
	if s, _, _, _ := store.ManualOverrideAction("cancelled", true); s != "reopen_pay" {
		t.Fatal(s)
	}
	if _, _, _, ok := store.ManualOverrideAction("cancelled", false); ok {
		t.Fatal("not reopenable")
	}
	if _, _, _, ok := store.ManualOverrideAction("refunded", false); ok {
		t.Fatal("refunded")
	}
}

func TestProductValidators(t *testing.T) {
	if c, err := store.ParseProductPrice("1.234,56"); err != nil || c != 123456 {
		t.Fatal(c, err)
	}
	if c, err := store.ParseProductPrice("199.90"); err != nil || c != 19990 {
		t.Fatal(c, err)
	}
	for _, bad := range []string{"", "0", "-1", "abc", "10000000"} {
		if _, err := store.ParseProductPrice(bad); err == nil {
			t.Error(bad)
		}
	}
	if _, err := store.ParseCompareAtPrice("100", 10000); err == nil {
		t.Fatal("compare below price")
	}
	if c, err := store.ParseCompareAtPrice("", 10000); err != nil || c != nil {
		t.Fatal(c, err)
	}
	for _, bad := range []string{"", "-a", "a-", "a--b", "A_B", strings.Repeat("a", 121)} {
		if _, err := store.ValidateProductSlug(bad); err == nil {
			t.Error(bad)
		}
	}
	if s, err := store.ValidateProductSlug(" Biquini-1 "); err != nil || s != "biquini-1" {
		t.Fatal(s, err)
	}
	if h, err := store.ValidateColorHex("#a1b2c3"); err != nil || *h != "#A1B2C3" {
		t.Fatal(h, err)
	}
	for _, bad := range []string{"a1b2c3", "#a1b2c", "#zzzzzz"} {
		if _, err := store.ValidateColorHex(bad); err == nil {
			t.Error(bad)
		}
	}
	if _, err := store.ParseSetCompare("100", nil); err == nil {
		t.Fatal("compare without price")
	}
	if _, err := store.ZipVariantForms([]string{"a"}, []string{"m"}, nil, nil, nil, nil, false); err == nil {
		t.Fatal("mismatched variant arrays")
	}
}

func TestValidateEmailAndName(t *testing.T) {
	for _, bad := range []string{"a.b@", "@x.com", "a b@x.com", "a@b@c.com", "a@x..com", ""} {
		if _, ok := store.ValidateEmail(bad); ok {
			t.Error(bad)
		}
	}
	if e, ok := store.ValidateEmail(" Ana@Example.COM "); !ok || e != "ana@example.com" {
		t.Fatal(e)
	}
	if _, ok := store.ValidateName(strings.Repeat("a", 101)); ok {
		t.Fatal("long name")
	}
}

func TestSubscribedMessageMentionsFirstPurchaseCoupon(t *testing.T) {
	if got := store.SubscribedMessage(" BEMVINDA10 "); got != "Inscrição realizada! Use o cupom BEMVINDA10 na sua primeira compra." {
		t.Fatal(got)
	}
	if got := store.SubscribedMessage(""); got != "Inscrição realizada! Você vai receber nossas novidades por e-mail." {
		t.Fatal(got)
	}
	for _, c := range []struct{ email, msg string }{{"", "E-mail é obrigatório"}, {"nope", "E-mail inválido"}} {
		if _, _, msg := store.ValidateNewsletterForm(c.email, ""); msg != c.msg {
			t.Fatal(msg)
		}
	}
	email, name, msg := store.ValidateNewsletterForm(" Ana@X.com ", strings.Repeat("n", 150))
	if msg != "" || email != "ana@x.com" || len([]rune(*name)) != 100 {
		t.Fatal(email, msg)
	}
}

// Port of src/app/newsletter.rs newsletter_uses_strict_email_validation:
// the old check (contains '@' and '.') accepted these.
func TestNewsletterUsesStrictEmailValidation(t *testing.T) {
	for _, bad := range []string{"a.b@", "@x.com", "a b@x.com", "a@b@c.com", "a@x..com"} {
		if _, _, msg := store.ValidateNewsletterForm(bad, ""); msg != "E-mail inválido" {
			t.Errorf("%q: msg = %q", bad, msg)
		}
	}
	if email, _, msg := store.ValidateNewsletterForm(" Ana@Example.COM ", ""); msg != "" || email != "ana@example.com" {
		t.Fatalf("email = %q, msg = %q", email, msg)
	}
}

func TestOrderCodeIsFirstEightHex(t *testing.T) {
	id := uuid.MustParse("0192f3a4-b5c6-7d8e-9f00-112233445566")
	if store.OrderCode(id) != "0192F3A4" {
		t.Fatal(store.OrderCode(id))
	}
}
