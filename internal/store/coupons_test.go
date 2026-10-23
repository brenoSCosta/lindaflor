package store_test

import (
	"errors"
	"fmt"
	"strings"
	"testing"
	"time"

	"github.com/google/uuid"
	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgconn"

	"github.com/wellington-vell/lindaflor/internal/store"
	"github.com/wellington-vell/lindaflor/internal/testutil"
)

// Port of the src/app/store/coupons.rs tests.

func coupon(discountType string, value, minSubtotal int64, maxDiscount *int64) *store.Coupon {
	return &store.Coupon{ID: newID(), Code: "TEST", DiscountType: discountType, DiscountValue: value,
		MinSubtotalCents: minSubtotal, MaxDiscountCents: maxDiscount, Active: true, UsageType: "unlimited", PerUserLimit: 1}
}

func TestFixedIsCappedAtSubtotal(t *testing.T) {
	c := coupon("fixed", 500, 0, nil)
	for sub, want := range map[int64]int64{300: 300, 500: 500, 1000: 500, 0: 0} {
		if got := store.CalcCouponDiscount(c, sub); got != want {
			t.Errorf("%d: %d", sub, got)
		}
	}
}

func TestPercentFloorsAndCaps(t *testing.T) {
	c := coupon("percent", 10, 0, nil)
	if store.CalcCouponDiscount(c, 999) != 99 || store.CalcCouponDiscount(c, 1000) != 100 || store.CalcCouponDiscount(c, 10000) != 1000 {
		t.Fatal("percent")
	}
	if got := store.CalcCouponDiscount(coupon("percent", 50, 0, price(200)), 10000); got != 200 {
		t.Fatal(got)
	}
	if got := store.CalcCouponDiscount(coupon("percent", 50, 0, nil), 100); got != 50 {
		t.Fatal(got)
	}
}

func TestRespectsMinSubtotal(t *testing.T) {
	c := coupon("fixed", 100, 1000, nil)
	if store.CalcCouponDiscount(c, 999) != 0 || store.CalcCouponDiscount(c, 1000) != 100 {
		t.Fatal("min subtotal")
	}
}

func TestUniqueImpliesSingleUse(t *testing.T) {
	c := coupon("fixed", 100, 0, nil)
	c.UsageType = "unique"
	if m := c.EffectiveMaxUses(); m == nil || *m != 1 {
		t.Fatal("unique")
	}
	c.UsageType = "unlimited"
	five := int32(5)
	c.MaxUses = &five
	if m := c.EffectiveMaxUses(); m == nil || *m != 5 {
		t.Fatal("max uses")
	}
	c.MaxUses = nil
	if c.EffectiveMaxUses() != nil {
		t.Fatal("unlimited")
	}
}

func TestNormalizeEmailFoldsAliases(t *testing.T) {
	if _, ok := store.NormalizeEmail("  "); ok {
		t.Fatal("blank")
	}
	for in, want := range map[string]string{
		" Maria@Example.COM ":        "maria@example.com",
		"maria+promo@example.com":    "maria@example.com",
		"ma.ria@example.com":         "ma.ria@example.com",
		"Ma.Ria+x@Gmail.com":         "maria@gmail.com",
		"m.a.r.i.a@googlemail.com":   "maria@gmail.com",
		"+tag@example.com":           "+tag@example.com",
		"NoAt":                       "noat",
	} {
		if got, _ := store.NormalizeEmail(in); got != want {
			t.Errorf("%q → %q, want %q", in, got, want)
		}
	}
}

func uniqueCode(prefix string) string {
	return prefix + "-" + strings.ReplaceAll(newID().String(), "-", "")
}

type newCoupon struct {
	code, discountType, usageType string
	value, minSubtotal            int64
	maxDiscount                   *int64
	active                        bool
	startsAt, expiresAt           *time.Time
	maxUses                       *int32
	perUser                       int32
}

func defaultCoupon(code string) newCoupon {
	return newCoupon{code: code, discountType: "fixed", value: 1000, active: true, usageType: "unlimited", perUser: 1}
}

func insertCoupon(t *testing.T, tx pgx.Tx, c newCoupon) uuid.UUID {
	t.Helper()
	id := newID()
	exec(t, tx, `INSERT INTO coupons (id, code, discount_type, discount_value, min_subtotal_cents,
		max_discount_cents, active, starts_at, expires_at, usage_type, max_uses, per_user_limit)
		VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)`,
		id, c.code, c.discountType, c.value, c.minSubtotal, c.maxDiscount, c.active, c.startsAt, c.expiresAt, c.usageType, c.maxUses, c.perUser)
	return id
}

func insertOrderFor(t *testing.T, tx pgx.Tx, userID *uuid.UUID) uuid.UUID {
	t.Helper()
	id := newID()
	exec(t, tx, `INSERT INTO orders (id, user_id, access_token) VALUES ($1, $2, $3)`, id, userID, newID())
	return id
}

func insertRedemption(t *testing.T, tx pgx.Tx, couponID uuid.UUID, userID *uuid.UUID, orderID uuid.UUID, guest *string) {
	t.Helper()
	var norm *string
	if guest != nil {
		n, _ := store.NormalizeEmail(*guest)
		norm = &n
	}
	exec(t, tx, `INSERT INTO coupon_redemptions (id, coupon_id, user_id, order_id, guest_email, email_norm) VALUES ($1, $2, $3, $4, $5, $6)`,
		newID(), couponID, userID, orderID, guest, norm)
}

func str(s string) *string { return &s }

func resolveOK(t *testing.T, tx pgx.Tx, code string, sub int64, user *uuid.UUID, guest *string) *store.ResolvedCoupon {
	t.Helper()
	r, err := store.ResolveCoupon(ctx, tx, code, sub, user, guest, false)
	if err != nil || r == nil {
		t.Fatalf("resolve %q: %v %v", code, r, err)
	}
	return r
}

func resolveInvalid(t *testing.T, tx pgx.Tx, code string, sub int64, user *uuid.UUID, guest *string) string {
	t.Helper()
	_, err := store.ResolveCoupon(ctx, tx, code, sub, user, guest, false)
	return userMsg(t, err)
}

func TestBlankCodeReturnsNone(t *testing.T) {
	pool := testutil.NewPool(t)
	tx := begin(t, pool)
	r, err := store.ResolveCoupon(ctx, tx, "   ", 5000, nil, str("g@x.co"), false)
	if r != nil || err != nil {
		t.Fatal(r, err)
	}
}

func TestUnknownOrInactiveCodeIsGone(t *testing.T) {
	pool := testutil.NewPool(t)
	tx := begin(t, pool)
	if _, err := store.ResolveCoupon(ctx, tx, uniqueCode("missing"), 5000, nil, str("g@x.co"), false); !errors.Is(err, store.ErrCouponGone) {
		t.Fatal(err)
	}
	code := uniqueCode("off")
	c := defaultCoupon(code)
	c.active = false
	insertCoupon(t, tx, c)
	if _, err := store.ResolveCoupon(ctx, tx, code, 5000, nil, str("g@x.co"), false); !errors.Is(err, store.ErrCouponGone) {
		t.Fatal(err)
	}
}

func TestFutureStartAndPastExpiryAreGenericInvalid(t *testing.T) {
	pool := testutil.NewPool(t)
	tx := begin(t, pool)
	future := time.Now().Add(time.Hour)
	past := time.Now().Add(-time.Hour)
	soon := defaultCoupon(uniqueCode("soon"))
	soon.startsAt = &future
	insertCoupon(t, tx, soon)
	if msg := resolveInvalid(t, tx, soon.code, 5000, nil, str("g@x.co")); msg != store.InvalidCouponMessage {
		t.Fatal(msg)
	}
	old := defaultCoupon(uniqueCode("old"))
	old.expiresAt = &past
	insertCoupon(t, tx, old)
	if msg := resolveInvalid(t, tx, old.code, 5000, nil, str("g@x.co")); msg != store.InvalidCouponMessage {
		t.Fatal(msg)
	}
}

func TestSubtotalUnderMinimumMentionsPrice(t *testing.T) {
	pool := testutil.NewPool(t)
	tx := begin(t, pool)
	c := defaultCoupon(uniqueCode("min"))
	c.minSubtotal = 15000
	insertCoupon(t, tx, c)
	want := fmt.Sprintf("Este cupom exige uma compra mínima de %s.", store.FormatPrice(15000))
	if msg := resolveInvalid(t, tx, c.code, 1000, nil, str("g@x.co")); msg != want {
		t.Fatal(msg)
	}
}

func TestFixedAndPercentReturnStoredCodeAndDiscount(t *testing.T) {
	pool := testutil.NewPool(t)
	tx := begin(t, pool)
	fixed := defaultCoupon(uniqueCode("Fixo"))
	fixed.value = 2500
	insertCoupon(t, tx, fixed)
	r := resolveOK(t, tx, strings.ToLower(fixed.code), 10000, nil, str("g@x.co"))
	if r.Code != fixed.code || r.DiscountCents != 2500 {
		t.Fatalf("%+v", r)
	}
	pct := defaultCoupon(uniqueCode("Pct"))
	pct.discountType, pct.value, pct.maxDiscount = "percent", 40, price(500)
	insertCoupon(t, tx, pct)
	r = resolveOK(t, tx, strings.ToLower(pct.code), 10000, nil, str("g@x.co"))
	if r.Code != pct.code || r.DiscountCents != 500 {
		t.Fatalf("%+v", r)
	}
}

func TestAssignmentsOpenForGuestAndRestrictedToMember(t *testing.T) {
	pool := testutil.NewPool(t)
	tx := begin(t, pool)
	open := defaultCoupon(uniqueCode("open"))
	insertCoupon(t, tx, open)
	resolveOK(t, tx, open.code, 5000, nil, str("g@x.co"))

	vip := defaultCoupon(uniqueCode("vip"))
	couponID := insertCoupon(t, tx, vip)
	member := insertUser(t, tx, "Test User")
	outsider := insertUser(t, tx, "Test User")
	exec(t, tx, `INSERT INTO coupon_assignments (coupon_id, user_id) VALUES ($1, $2)`, couponID, member)
	if msg := resolveInvalid(t, tx, vip.code, 5000, nil, str("g@x.co")); msg != store.InvalidCouponMessage {
		t.Fatal(msg)
	}
	if msg := resolveInvalid(t, tx, vip.code, 5000, &outsider, nil); msg != store.InvalidCouponMessage {
		t.Fatal(msg)
	}
	exec(t, tx, `UPDATE coupons SET min_subtotal_cents = 50000 WHERE id = $1`, couponID)
	if msg := resolveInvalid(t, tx, vip.code, 100, &outsider, nil); msg != store.InvalidCouponMessage {
		t.Fatal("outsider below minimum must still get the generic reject: " + msg)
	}
	if msg := resolveInvalid(t, tx, vip.code, 100, &member, nil); !strings.Contains(msg, "compra mínima") {
		t.Fatal(msg)
	}
	exec(t, tx, `UPDATE coupons SET min_subtotal_cents = 0 WHERE id = $1`, couponID)
	resolveOK(t, tx, vip.code, 5000, &member, nil)
}

func TestGlobalCapUniqueUnlimitedAndNull(t *testing.T) {
	pool := testutil.NewPool(t)
	tx := begin(t, pool)
	once := defaultCoupon(uniqueCode("once"))
	once.usageType = "unique"
	onceID := insertCoupon(t, tx, once)
	insertRedemption(t, tx, onceID, nil, insertOrderFor(t, tx, nil), str("g@x.co"))
	if msg := resolveInvalid(t, tx, once.code, 5000, nil, str("g@x.co")); msg != "Este cupom atingiu o limite de utilizações." {
		t.Fatal(msg)
	}
	capped := defaultCoupon(uniqueCode("cap"))
	two := int32(2)
	capped.maxUses = &two
	cappedID := insertCoupon(t, tx, capped)
	for range 2 {
		insertRedemption(t, tx, cappedID, nil, insertOrderFor(t, tx, nil), str("g@x.co"))
	}
	if msg := resolveInvalid(t, tx, capped.code, 5000, nil, str("g@x.co")); msg != "Este cupom atingiu o limite de utilizações." {
		t.Fatal(msg)
	}
	open := defaultCoupon(uniqueCode("nocap"))
	open.perUser = 10
	openID := insertCoupon(t, tx, open)
	for range 3 {
		insertRedemption(t, tx, openID, nil, insertOrderFor(t, tx, nil), str("g@x.co"))
	}
	resolveOK(t, tx, open.code, 5000, nil, str("g@x.co"))
}

func TestPerUserCapKeysGuestEmailAndRequiresIdentity(t *testing.T) {
	pool := testutil.NewPool(t)
	tx := begin(t, pool)
	c := defaultCoupon(uniqueCode("personal"))
	couponID := insertCoupon(t, tx, c)
	user := insertUser(t, tx, "Test User")
	insertRedemption(t, tx, couponID, &user, insertOrderFor(t, tx, &user), nil)
	const limit = "Você já utilizou este cupom o máximo de vezes permitido."
	if msg := resolveInvalid(t, tx, c.code, 5000, &user, nil); msg != limit {
		t.Fatal(msg)
	}
	if msg := resolveInvalid(t, tx, c.code, 5000, nil, nil); msg != "Entre para usar este cupom, ou informe seu e-mail no checkout." {
		t.Fatal(msg)
	}
	guestOrder := insertOrderFor(t, tx, nil)
	insertRedemption(t, tx, couponID, nil, guestOrder, str("guest@x.co"))
	if msg := resolveInvalid(t, tx, c.code, 5000, nil, str("guest@x.co")); msg != limit {
		t.Fatal(msg)
	}
	resolveOK(t, tx, c.code, 5000, nil, str("other@x.co"))
	mustNoErr(t, store.RestoreRedemptionForOrder(ctx, tx, guestOrder))
	resolveOK(t, tx, c.code, 5000, nil, str("guest@x.co"))
}

func TestPerUserCapFollowsEmailAcrossLoginAndAliases(t *testing.T) {
	pool := testutil.NewPool(t)
	tx := begin(t, pool)
	c := defaultCoupon(uniqueCode("Primeira"))
	insertCoupon(t, tx, c)
	local := "maria." + strings.ReplaceAll(newID().String(), "-", "")
	user := newID()
	exec(t, tx, `INSERT INTO users (id, name, email) VALUES ($1, 'Maria', $2)`, user, local+"@Gmail.com")
	r, err := store.ResolveCoupon(ctx, tx, c.code, 5000, &user, str("whatever@example.com"), true)
	if err != nil || r == nil {
		t.Fatal(err)
	}
	orderID := insertOrderFor(t, tx, &user)
	mustNoErr(t, store.RecordRedemption(ctx, tx, r, &user, orderID, nil))
	canonical := strings.ReplaceAll(local, ".", "") + "@gmail.com"
	if got := scalar[string](t, tx, `SELECT email_norm FROM coupon_redemptions WHERE order_id = $1`, orderID); got != canonical {
		t.Fatal(got)
	}
	dotless, rest, _ := strings.Cut(local, ".")
	for _, guest := range []string{
		local + "@gmail.com", strings.ToUpper(local) + "@GMAIL.com", local + "+promo@gmail.com",
		dotless + rest + "@googlemail.com", dotless + "." + rest + "+x@googlemail.com",
	} {
		if msg := resolveInvalid(t, tx, c.code, 5000, nil, &guest); msg != "Você já utilizou este cupom o máximo de vezes permitido." {
			t.Errorf("%s: %s", guest, msg)
		}
	}
	other := "outra." + strings.ReplaceAll(newID().String(), "-", "") + "@gmail.com"
	resolveOK(t, tx, c.code, 5000, nil, &other)
}

func TestFirstPurchaseCodeRequiresNoPaidOrders(t *testing.T) {
	pool := testutil.NewPool(t)
	tx := begin(t, pool)
	c := defaultCoupon(uniqueCode("BEMVINDA"))
	c.perUser = 5
	insertCoupon(t, tx, c)
	other := defaultCoupon(uniqueCode("other"))
	other.perUser = 5
	insertCoupon(t, tx, other)
	exec(t, tx, `INSERT INTO store_settings (id, first_purchase_coupon_code) VALUES (true, $1)
		ON CONFLICT (id) DO UPDATE SET first_purchase_coupon_code = EXCLUDED.first_purchase_coupon_code`, strings.ToLower(c.code))
	user := insertUser(t, tx, "Test User")
	insertOrderFor(t, tx, &user)
	resolveOK(t, tx, c.code, 5000, &user, nil)
	paid := insertOrderFor(t, tx, &user)
	exec(t, tx, `UPDATE orders SET paid_at = now() WHERE id = $1`, paid)
	if msg := resolveInvalid(t, tx, c.code, 5000, &user, nil); msg != store.FirstPurchaseOnlyMessage {
		t.Fatal(msg)
	}
	accountEmail := strings.ToUpper("u-" + user.String() + "@example.com")
	if msg := resolveInvalid(t, tx, c.code, 5000, nil, &accountEmail); msg != store.FirstPurchaseOnlyMessage {
		t.Fatal(msg)
	}
	resolveOK(t, tx, other.code, 5000, &user, nil)
	guestOrder := insertOrderFor(t, tx, nil)
	exec(t, tx, `UPDATE orders SET paid_at = now(), guest_email = 'Buyer@x.co' WHERE id = $1`, guestOrder)
	if msg := resolveInvalid(t, tx, c.code, 5000, nil, str("buyer@x.co")); msg != store.FirstPurchaseOnlyMessage {
		t.Fatal(msg)
	}
	resolveOK(t, tx, c.code, 5000, nil, str("new@x.co"))
	// The dashboard offer follows the same rule.
	if offer, _ := store.FirstPurchaseOffer(ctx, tx, user, "u-"+user.String()+"@example.com"); offer != "" {
		t.Fatal("offer for a buyer")
	}
	fresh := insertUser(t, tx, "Nova")
	if offer, _ := store.FirstPurchaseOffer(ctx, tx, fresh, "u-"+fresh.String()+"@example.com"); !strings.EqualFold(offer, c.code) {
		t.Fatal(offer)
	}
}

func TestCodeIsUniqueCaseInsensitively(t *testing.T) {
	pool := testutil.NewPool(t)
	tx := begin(t, pool)
	c := defaultCoupon(uniqueCode("Dup"))
	insertCoupon(t, tx, c)
	_, err := tx.Exec(ctx, `INSERT INTO coupons (id, code, discount_type, discount_value) VALUES ($1, $2, 'fixed', 100)`, newID(), strings.ToLower(c.code))
	var pg *pgconn.PgError
	if !errors.As(err, &pg) || pg.Code != "23505" {
		t.Fatalf("case-variant code must collide: %v", err)
	}
}

func TestForUpdateResolvesInsideTransaction(t *testing.T) {
	pool := testutil.NewPool(t)
	tx := begin(t, pool)
	c := defaultCoupon(uniqueCode("lock"))
	insertCoupon(t, tx, c)
	r, err := store.ResolveCoupon(ctx, tx, c.code, 8000, nil, str("g@x.co"), true)
	if err != nil || r == nil || r.DiscountCents != 1000 {
		t.Fatal(r, err)
	}
}

func TestAdminCouponCRUD(t *testing.T) {
	pool := testutil.NewPool(t)
	for _, bad := range []store.CouponForm{
		{Code: "", DiscountType: "fixed", DiscountValue: "100", UsageType: "unlimited"},
		{Code: strings.Repeat("A", 33), DiscountType: "fixed", DiscountValue: "100", UsageType: "unlimited"},
		{Code: "X", DiscountType: "bogus", DiscountValue: "100", UsageType: "unlimited"},
		{Code: "X", DiscountType: "percent", DiscountValue: "51", UsageType: "unlimited"},
		{Code: "X", DiscountType: "fixed", DiscountValue: "50001", UsageType: "unlimited"},
		{Code: "X", DiscountType: "fixed", DiscountValue: "100", UsageType: "unlimited", MaxUses: "0"},
		{Code: "X", DiscountType: "fixed", DiscountValue: "100", UsageType: "weird"},
		{Code: "X", DiscountType: "fixed", DiscountValue: "100", UsageType: "unlimited", StartsAt: "2026-02-01T00:00", ExpiresAt: "2026-01-01T00:00"},
	} {
		if err := store.CreateCoupon(ctx, pool, bad); !store.IsUserError(err) {
			t.Errorf("%+v: %v", bad, err)
		}
	}
	mustNoErr(t, store.CreateCoupon(ctx, pool, store.CouponForm{Code: " verao ", DiscountType: "percent", DiscountValue: "10", UsageType: "unique", ExpiresAt: "2030-01-01T00:00"}))
	if err := store.CreateCoupon(ctx, pool, store.CouponForm{Code: "VERAO", DiscountType: "fixed", DiscountValue: "100", UsageType: "unlimited"}); userMsg(t, err) != "Este código já está em uso." {
		t.Fatal(err)
	}
	rows, err := store.ListAdminCoupons(ctx, pool, "ver")
	mustNoErr(t, err)
	if len(rows) != 1 || rows[0].Code != "VERAO" || rows[0].DiscountLabel != "10%" || rows[0].UsageLabel != "Único" || !strings.HasPrefix(rows[0].Validity, "até 2030-01-01") {
		t.Fatalf("%+v", rows)
	}
	id := rows[0].ID
	mustNoErr(t, store.ToggleCoupon(ctx, pool, id))
	user := insertUser(t, pool, "Ana")
	if err := store.AssignCoupon(ctx, pool, id, []string{"junk"}); userMsg(t, err) != "Selecione ao menos um usuário." {
		t.Fatal(err)
	}
	mustNoErr(t, store.AssignCoupon(ctx, pool, id, []string{user.String()}))
	assigned, err := store.CouponAssignees(ctx, pool, id)
	mustNoErr(t, err)
	if len(assigned) != 1 || assigned[0].ID != user {
		t.Fatalf("%+v", assigned)
	}
	mustNoErr(t, store.UnassignCoupon(ctx, pool, id, user))
	mustNoErr(t, store.DeleteCoupon(ctx, pool, id))
	if err := store.DeleteCoupon(ctx, pool, id); userMsg(t, err) != "Cupom não encontrado." {
		t.Fatal(err)
	}
	if got := store.CouponsURL("a b", "c", "d"); got != "/admin/cupons?q=a+b&cupom=c&uq=d" {
		t.Fatal(got)
	}
	if got := store.CouponsURL("", "", "d"); got != "/admin/cupons" {
		t.Fatal(got)
	}
}
