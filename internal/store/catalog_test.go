package store_test

import (
	"errors"
	"net/http"
	"net/url"
	"strings"
	"sync"
	"testing"

	"github.com/google/uuid"

	"github.com/wellington-vell/lindaflor/internal/storage"
	"github.com/wellington-vell/lindaflor/internal/store"
	"github.com/wellington-vell/lindaflor/internal/testutil"
)

// Catalog/queries.rs, reviews DB, pedido/id.rs, admin data and login-hook
// tests.

func TestSearchTreatsLikeWildcardsLiterally(t *testing.T) {
	pool := testutil.NewPool(t)
	for _, name := range []string{"Top 100% linho", "Top 1000 linho", "tag_a", "tagXa"} {
		insertProduct(t, pool, name, 1000, true)
	}
	for term, want := range map[string]int64{"100%": 1, "%": 1, "tag_a": 1, "_": 1, "linho": 2} {
		q := store.CatalogQuery{Search: term, IncludeOutOfStock: true, Limit: 10}
		total, err := store.CountProducts(ctx, pool, q)
		mustNoErr(t, err)
		rows, err := store.ListProducts(ctx, pool, q)
		mustNoErr(t, err)
		if total != want || int64(len(rows)) != want {
			t.Errorf("%q: count %d list %d, want %d", term, total, len(rows), want)
		}
	}
}

func TestListProductsRespectsLimit(t *testing.T) {
	pool := testutil.NewPool(t)
	for range 3 {
		insertProduct(t, pool, "P", 1000, true)
	}
	page, err := store.ListProducts(ctx, pool, store.CatalogQuery{IncludeOutOfStock: true, Limit: 2})
	mustNoErr(t, err)
	if len(page) != 2 {
		t.Fatal(len(page))
	}
	if total, _ := store.CountProducts(ctx, pool, store.CatalogQuery{IncludeOutOfStock: true}); total < 3 {
		t.Fatal(total)
	}
}

func TestCatalogFiltersSortsAndFacets(t *testing.T) {
	pool := testutil.NewPool(t)
	wh := insertWarehouse(t, pool, true, true)
	mk := func(name, cat string, price int64, compare *int64, size, color string, stock int32) uuid.UUID {
		id := newID()
		exec(t, pool, `INSERT INTO products (id, name, slug, price_in_cents, compare_at_price_in_cents, category) VALUES ($1, $2, $3, $4, $5, $6::text::product_category)`,
			id, name, "p-"+id.String(), price, compare, cat)
		v := insertVariant(t, pool, id, size, color)
		exec(t, pool, `UPDATE product_variants SET color_hex = '#112233' WHERE id = $1`, v)
		insertInventory(t, pool, v, wh, stock, 0)
		exec(t, pool, `INSERT INTO product_images (id, product_id, url, sort_order) VALUES ($1, $2, 'products/x/a.jpg', 0), ($3, $2, 'https://img.example/b.jpg', 1)`, newID(), id, newID())
		return id
	}
	cheap := mk("Biquíni Barato", "biquini", 5000, nil, "p", "Azul", 3)
	sale := mk("Maiô Promo", "maio", 8000, price(16000), "m", "Verde", 2)
	soldOut := mk("Biquíni Esgotado", "biquini", 9000, nil, "g", "Azul", 0)

	page, err := store.BrowseCatalog(ctx, pool, "")
	mustNoErr(t, err)
	if page.Pager.Total != 2 || page.Heading != "Catálogo" {
		t.Fatalf("%+v", page.Pager)
	}
	page, _ = store.BrowseCatalog(ctx, pool, "esgotados=true&sort=menor_preco")
	if len(page.Products) != 3 || page.Products[0].ID != cheap || page.Heading != "Catálogo" {
		t.Fatalf("%+v", page.Products)
	}
	page, _ = store.BrowseCatalog(ctx, pool, "esgotados=true&sort=maior_preco")
	if page.Products[0].ID != soldOut || !page.Products[0].SoldOut() {
		t.Fatal("maior preço")
	}
	page, _ = store.BrowseCatalog(ctx, pool, "sale=true&sort=maior_desconto")
	if len(page.Products) != 1 || page.Products[0].ID != sale || page.Heading != "Liquida" || page.Products[0].DiscountBadge() != "-50%" {
		t.Fatalf("%+v", page.Products)
	}
	page, _ = store.BrowseCatalog(ctx, pool, "category=biquini&esgotados=true&cor=Azul&tamanho=p")
	if len(page.Products) != 1 || page.Products[0].ID != cheap || len(page.AvailableColors) != 1 || page.AvailableColors[0].Name != "Azul" {
		t.Fatalf("%+v %+v", page.Products, page.AvailableColors)
	}
	page, _ = store.BrowseCatalog(ctx, pool, "esgotados=true&preco_min=60&preco_max=85,50")
	if len(page.Products) != 1 || page.Products[0].ID != sale {
		t.Fatalf("%+v", page.Products)
	}
	page, _ = store.BrowseCatalog(ctx, pool, "esgotados=true&disponivel=true&search=biquíni")
	if len(page.Products) != 1 || page.Products[0].ID != cheap || page.Heading != "Busca: biquíni" {
		t.Fatalf("%+v", page.Products)
	}
	p := page.Products[0]
	if p.ImageURL == nil || p.SecondaryImageURL == nil || len(p.Colors) != 1 || *p.Colors[0].Hex != "#112233" || p.CategoryLabel() != "Biquíni" {
		t.Fatalf("%+v", p)
	}
	mem := storage.NewMemory()
	mustNoErr(t, mem.Put(ctx, "products/x/a.jpg", []byte("x"), "image/jpeg"))
	resolved := store.ResolveSummaryImages(ctx, mem, []store.ProductSummary{p})
	if *resolved[0].ImageURL != "memory://products/x/a.jpg" || *resolved[0].SecondaryImageURL != "https://img.example/b.jpg" {
		t.Fatalf("%v %v", *resolved[0].ImageURL, *resolved[0].SecondaryImageURL)
	}
	detail, err := store.GetProductBySlug(ctx, pool, "p-"+cheap.String())
	mustNoErr(t, err)
	if detail == nil || len(detail.Variants) != 1 || detail.Variants[0].Available != 3 || len(detail.Images) != 2 {
		t.Fatalf("%+v", detail)
	}
	store.ResolveDetailImages(ctx, storage.Unavailable{}, detail)
	if len(detail.Images) != 1 {
		t.Fatal("unresolvable key dropped")
	}
	if missing, _ := store.GetProductBySlug(ctx, pool, "nope"); missing != nil {
		t.Fatal("missing")
	}
	related, err := store.ListRelatedProducts(ctx, pool, cheap, "biquini")
	mustNoErr(t, err)
	if len(related) != 1 || related[0].ID != soldOut {
		t.Fatalf("%+v", related)
	}
	home, err := store.LoadHomeSections(ctx, pool)
	mustNoErr(t, err)
	if len(home.Lancamentos) != 3 || len(home.Liquida) != 1 || home.HasBanners {
		t.Fatalf("%+v", home)
	}
}

func TestSetsCatalogAndAvailability(t *testing.T) {
	pool := testutil.NewPool(t)
	setID, variants := seedSet(t, pool, price(10000), []int64{6000, 7000}, []int32{2, 5})
	for i, v := range variants {
		pid := scalar[uuid.UUID](t, pool, `SELECT product_id FROM product_variants WHERE id = $1`, v)
		exec(t, pool, `INSERT INTO set_members (set_id, product_id, sort_order) VALUES ($1, $2, $3)`, setID, pid, int32(i)) //nolint:gosec // tiny
	}
	sets, err := store.ListSets(ctx, pool)
	mustNoErr(t, err)
	if len(sets) != 1 || sets[0].ProductCount != 2 {
		t.Fatalf("%+v", sets)
	}
	set, err := store.GetSetBySlug(ctx, pool, sets[0].Slug)
	mustNoErr(t, err)
	if set == nil || set.ID != setID {
		t.Fatal("by slug")
	}
	if n, _ := store.SetAvailableQuantity(ctx, pool, setID); n != 2 {
		t.Fatal(n)
	}
	members, err := store.ListSetMembers(ctx, pool, setID)
	mustNoErr(t, err)
	if len(members) != 2 {
		t.Fatal(len(members))
	}
	nav, _ := store.NavSets(ctx, pool)
	if len(nav) != 1 {
		t.Fatal(len(nav))
	}
}

func TestWishlistToggle(t *testing.T) {
	pool := testutil.NewPool(t)
	user := insertUser(t, pool, "Ana")
	product := insertProduct(t, pool, "P", 1000, true)
	inactive := insertProduct(t, pool, "Off", 1000, false)
	if liked, err := store.ToggleWishlist(ctx, pool, user, product); err != nil || !liked {
		t.Fatal(liked, err)
	}
	if liked, _ := store.ToggleWishlist(ctx, pool, user, inactive); liked {
		t.Fatal("inactive products can't be liked")
	}
	ids, _ := store.WishlistIDs(ctx, pool, user)
	list, err := store.ListWishlistProducts(ctx, pool, user)
	mustNoErr(t, err)
	if !ids[product] || len(list) != 1 {
		t.Fatal(ids, list)
	}
	if liked, _ := store.ToggleWishlist(ctx, pool, user, product); liked || store.WishlistToggleMessage(liked) != "Removido dos favoritos." {
		t.Fatal("unlike")
	}
}

func TestReviewsFlow(t *testing.T) {
	pool := testutil.NewPool(t)
	productID, variantID, _ := seedVariant(t, pool, 1000, 5)
	user := insertUser(t, pool, "Maria da Silva")
	r, _ := store.ParseNewReview("5", "Ótimo", "Amei", str("ideal"))
	if _, err := store.SubmitReview(ctx, pool, user, productID, r); !errors.Is(err, store.ErrReviewNotEligible) {
		t.Fatal(err)
	}
	orderID := newID()
	exec(t, pool, `INSERT INTO orders (id, user_id, status, shipped_at) VALUES ($1, $2, 'shipped', now() - interval '10 days')`, orderID, user)
	exec(t, pool, `INSERT INTO order_items (id, order_id, variant_id, product_name, variant_label, quantity, unit_price_cents) VALUES ($1, $2, $3, 'P', 'M', 1, 1000)`, newID(), orderID, variantID)
	if o, _ := store.EligibleOrder(ctx, pool, user, productID); o != nil {
		t.Fatal("shipped 10 days ago is not received yet")
	}
	exec(t, pool, `UPDATE orders SET shipped_at = now() - interval '22 days' WHERE id = $1`, orderID)
	if ids, _ := store.ReviewableOrderIDs(ctx, pool, user); !ids[orderID] {
		t.Fatal("reviewable order")
	}
	if ps, _ := store.ReviewableProductsForOrder(ctx, pool, user, orderID); len(ps) != 1 || ps[0].ProductID != productID {
		t.Fatal(ps)
	}
	id, err := store.SubmitReview(ctx, pool, user, productID, r)
	mustNoErr(t, err)
	if _, err := store.SubmitReview(ctx, pool, user, productID, r); !errors.Is(err, store.ErrAlreadyReviewed) || store.ReviewErrorMessage(err) != "Você já avaliou este produto." {
		t.Fatal(err)
	}
	sec, err := store.LoadReviewSection(ctx, pool, productID, &user, 1)
	mustNoErr(t, err)
	if sec.ShowForm || sec.Summary.Count != 0 || sec.ReviewedNote == "" {
		t.Fatalf("%+v", sec)
	}
	if n, _ := store.CountPendingReviews(ctx, pool); n != 1 {
		t.Fatal(n)
	}
	admin := insertUser(t, pool, "Admin")
	if found, err := store.ModerateReview(ctx, pool, id, true, admin, "Admin"); err != nil || !found {
		t.Fatal(found, err)
	}
	if found, _ := store.ModerateReview(ctx, pool, newID(), true, admin, "Admin"); found {
		t.Fatal("unknown review")
	}
	if got := scalar[string](t, pool, `SELECT metadata->>'action' FROM audit_events WHERE entity_id = $1`, id); got != "review.approve" {
		t.Fatal(got)
	}
	sec, _ = store.LoadReviewSection(ctx, pool, productID, nil, 1)
	if sec.Summary.Count != 1 || len(sec.Reviews) != 1 || sec.Reviews[0].Author() != "Maria S." || sec.Reviews[0].FitLabel() != "Tamanho ideal" {
		t.Fatalf("%+v", sec)
	}
	agg, _ := store.LoadAggregateRating(ctx, pool, productID)
	all, _ := store.AllRatings(ctx, pool)
	if agg == nil || agg.RatingAverage != 5 || all[productID].Count != 1 {
		t.Fatal(agg, all)
	}
	queue, _ := store.ListReviewsForAdmin(ctx, pool, store.AdminReviewsLimit)
	if tone, label := queue[0].StatusBadge(); len(queue) != 1 || label != "Aprovada" || tone != store.TonePrimary {
		t.Fatal(queue)
	}
}

func TestCustomerOrderPageAndCancel(t *testing.T) {
	pool := testutil.NewPool(t)
	orderID, variantID := seedOrderWithStock(t, pool, 2)
	token := scalar[uuid.UUID](t, pool, `SELECT access_token FROM orders WHERE id = $1`, orderID)
	tok := token.String()
	wrong := uuid.NewString()
	o, err := store.GetOrderForViewer(ctx, pool, orderID, &tok, nil)
	mustNoErr(t, err)
	if o == nil || o.AccessToken != token || len(o.Items) != 1 || o.Progress.Status != "pending_payment" || o.StatusLabel() != "Aguardando pagamento" {
		t.Fatalf("%+v", o)
	}
	if o, _ := store.GetOrderForViewer(ctx, pool, orderID, &wrong, nil); o != nil {
		t.Fatal("wrong token")
	}
	if s, _ := store.RefreshOrderStatus(ctx, pool, orderID.String(), tok, nil); s != "pending_payment" {
		t.Fatal(s)
	}
	if s, _ := store.RefreshOrderStatus(ctx, pool, orderID.String(), wrong, nil); s != "unknown" {
		t.Fatal(s)
	}
	if s, _ := store.RefreshOrderStatus(ctx, pool, "junk", tok, nil); s != "unknown" {
		t.Fatal(s)
	}
	if out, _ := store.CancelUnpaidOrder(ctx, pool, orderID, &wrong, nil); out != store.CustomerCancelNotFound {
		t.Fatal(out)
	}
	out, err := store.CancelUnpaidOrder(ctx, pool, orderID, &tok, nil)
	mustNoErr(t, err)
	if msg, ok := out.Message(); out != store.CustomerCancelled || !ok || msg != "Pedido cancelado." {
		t.Fatal(out)
	}
	if _, r := stockOf(t, pool, variantID); r != 0 {
		t.Fatal("hold released")
	}
	if got := scalar[string](t, pool, `SELECT actor_name FROM audit_events WHERE entity_id = $1`, orderID); got != "cliente" {
		t.Fatal(got)
	}
	paid, _ := seedOrderWithStock(t, pool, 2)
	tx := begin(t, pool)
	apply(t, tx, paid, store.EventPay, store.ApplyOptions{})
	commit(t, tx)
	ptok := scalar[uuid.UUID](t, pool, `SELECT access_token FROM orders WHERE id = $1`, paid).String()
	if out, _ := store.CancelUnpaidOrder(ctx, pool, paid, &ptok, nil); out != store.CustomerCancelNotPending {
		t.Fatal(out)
	}
}

func TestTokenExpiryDefaultsTo90DaysAndHidesOrder(t *testing.T) {
	pool := testutil.NewPool(t)
	id := newID()
	exec(t, pool, `INSERT INTO orders (id, guest_email, status, total_cents) VALUES ($1, 'guest@example.com', 'pending_payment', 1000)`, id)
	days := scalar[float64](t, pool, `SELECT EXTRACT(EPOCH FROM access_token_expires_at - now())::float8 / 86400 FROM orders WHERE id = $1`, id)
	if days < 89.9 || days > 90.1 {
		t.Fatal(days)
	}
	exec(t, pool, `UPDATE orders SET access_token_expires_at = now() - interval '1 minute' WHERE id = $1`, id)
	tok := scalar[uuid.UUID](t, pool, `SELECT access_token FROM orders WHERE id = $1`, id).String()
	o, _ := store.GetOrder(ctx, pool, id)
	if !o.TokenExpired {
		t.Fatal("expired")
	}
	if o, _ := store.GetOrderForViewer(ctx, pool, id, &tok, nil); o != nil {
		t.Fatal("expired link hides the order")
	}
	if out, _ := store.CancelUnpaidOrder(ctx, pool, id, &tok, nil); out != store.CustomerCancelNotFound {
		t.Fatal(out)
	}
}

func TestMyOrdersAndDashboard(t *testing.T) {
	pool := testutil.NewPool(t)
	user := insertUser(t, pool, "Ana")
	for range 25 {
		exec(t, pool, `INSERT INTO orders (id, user_id, status) VALUES ($1, $2, 'paid')`, newID(), user)
	}
	page, err := store.LoadMyOrders(ctx, pool, user, 9)
	mustNoErr(t, err)
	if page.Pager.Page != 2 || len(page.Orders) != 5 || page.Pager.Label() != "Página 2 de 2" || !page.Pager.HasPrev() || page.Pager.HasNext() {
		t.Fatalf("%+v", page.Pager)
	}
	recent, _ := store.LoadRecentOrders(ctx, pool, user)
	if len(recent) != 5 || recent[0].StatusTone() != store.TonePrimary {
		t.Fatal(len(recent))
	}
	if store.OrderWhatsappURL(store.DefaultStoreSettings(), newID(), 1000) != "" {
		t.Fatal("no number")
	}
}

func TestSettingsSaveAndSingleton(t *testing.T) {
	pool := testutil.NewPool(t)
	s, err := store.GetStoreSettings(ctx, pool)
	mustNoErr(t, err)
	if s.PixDiscountPercent != 5 || s.FreeShippingThresholdCents != 29900 || s.PixConfigured() {
		t.Fatalf("%+v", s)
	}
	form := store.SettingsFormFrom(s)
	if form.PixKeyType != "phone" || form.FreeShippingThreshold != "299.00" {
		t.Fatalf("%+v", form)
	}
	for _, mut := range []func(*store.SettingsForm){
		func(f *store.SettingsForm) { f.PixKey = "79999816511"; f.PixKeyType = "" },
		func(f *store.SettingsForm) { f.PixKey = "123"; f.PixKeyType = "cpf" },
		func(f *store.SettingsForm) { f.PixKeyType = "bogus" },
		func(f *store.SettingsForm) { f.PixDiscountPercent = "31" },
		func(f *store.SettingsForm) { f.PixMerchantCity = strings.Repeat("c", 16) },
		func(f *store.SettingsForm) { f.FreeShippingThreshold = "abc" },
	} {
		f := store.SettingsForm{}
		mut(&f)
		if err := store.SaveSettings(ctx, pool, f); !store.IsUserError(err) {
			t.Errorf("%+v: %v", f, err)
		}
	}
	mustNoErr(t, store.SaveSettings(ctx, pool, store.SettingsForm{PixKey: "(79) 99981-6511", PixKeyType: "phone", PixMerchantName: "Linda Flor", FreeShippingThreshold: "199,90", PixDiscountPercent: "10"}))
	s, _ = store.GetStoreSettings(ctx, pool)
	if *s.PixKey != "+5579999816511" || s.FreeShippingThresholdCents != 19990 || s.PixDiscountPercent != 10 || !s.PixConfigured() {
		t.Fatalf("%+v", s)
	}
	var wg sync.WaitGroup
	for _, city := range []string{"Aracaju", "Estância", "Lagarto", "Itabaiana"} {
		wg.Go(func() {
			if err := store.SaveSettings(ctx, pool, store.SettingsForm{PixKey: "79999816511", PixKeyType: "phone", PixMerchantCity: city}); err != nil {
				t.Error(err)
			}
		})
	}
	wg.Wait()
	if n := scalar[int64](t, pool, `SELECT COUNT(*) FROM store_settings`); n != 1 {
		t.Fatal(n)
	}
	if _, err := pool.Exec(ctx, `INSERT INTO store_settings (id) VALUES (false)`); err == nil {
		t.Fatal("singleton check")
	}
}

func TestFailedSetSaveKeepsPreviousSet(t *testing.T) {
	pool := testutil.NewPool(t)
	productID, variantID, _ := seedVariant(t, pool, 10000, 1)
	setID := newID()
	slug := "conjunto-" + setID.String()
	exec(t, pool, `INSERT INTO sets (id, name, slug, price_in_cents) VALUES ($1, 'Antes', $2, 15000)`, setID, slug)
	mustNoErr(t, store.WriteSet(ctx, pool, setID, store.SetWrite{
		Name: "Antes", Slug: slug, Price: price(15000), Active: true,
		MemberIDs: []uuid.UUID{productID}, Slots: []store.SetSlot{{Label: str("Top"), VariantID: variantID}},
	}))
	err := store.WriteSet(ctx, pool, setID, store.SetWrite{
		Name: "Depois", Slug: slug, Description: str("nova"), Price: price(20000),
		Slots: []store.SetSlot{{Label: str("Top novo"), VariantID: variantID}, {Label: str("Calcinha"), VariantID: newID()}},
	})
	if err == nil {
		t.Fatal("save with a missing variant must fail")
	}
	var name string
	var p *int64
	var active bool
	mustNoErr(t, pool.QueryRow(ctx, `SELECT name, price_in_cents, active FROM sets WHERE id = $1`, setID).Scan(&name, &p, &active))
	if name != "Antes" || *p != 15000 || !active {
		t.Fatal(name, *p, active)
	}
	if m := scalar[uuid.UUID](t, pool, `SELECT product_id FROM set_members WHERE set_id = $1`, setID); m != productID {
		t.Fatal("members")
	}
	if l := scalar[string](t, pool, `SELECT s.label FROM set_slots s JOIN set_slot_options o ON o.slot_id = s.id WHERE s.set_id = $1`, setID); l != "Top" {
		t.Fatal(l)
	}
}

func pngUpload() store.Upload {
	return store.Upload{ContentType: "image/png", Bytes: append([]byte{0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A}, 1, 2, 3)}
}

func TestAdminProductCreateAndUpdate(t *testing.T) {
	pool := testutil.NewPool(t)
	mem := storage.NewMemory()
	form := store.ProductForm{
		Name: "Biquíni Novo", Slug: "Biquini-Novo", Price: "199,90", CompareAtPrice: "249.90", Category: "biquini", Featured: true,
		Variants: []store.VariantForm{{SKU: "BN-P", Size: "P", Color: "Azul", ColorHex: "#0a9fb5", Quantity: "4"}},
		Images:   []store.Upload{pngUpload()},
	}
	res, err := store.CreateProduct(ctx, pool, mem, form)
	mustNoErr(t, err)
	if res.ImageFailed || res.Message() != "Produto criado com sucesso." {
		t.Fatal(res)
	}
	if _, err := store.CreateProduct(ctx, pool, mem, form); userMsg(t, err) != "Este slug já está em uso por outro produto." {
		t.Fatal(err)
	}
	edit, err := store.LoadAdminProduct(ctx, pool, mem, res.ID)
	mustNoErr(t, err)
	if edit.Slug != "biquini-novo" || edit.PriceInput() != "199.90" || edit.CompareAtInput() != "249.90" || len(edit.Variants) != 1 ||
		len(edit.Images) != 1 || !strings.HasPrefix(edit.Images[0].URL, "memory://products") {
		t.Fatalf("%+v", edit)
	}
	if q := scalar[int32](t, pool, `SELECT quantity FROM inventory i JOIN product_variants v ON v.id = i.variant_id WHERE v.sku = 'BN-P'`); q != 4 {
		t.Fatal(q)
	}
	other, err := store.CreateProduct(ctx, pool, mem, store.ProductForm{
		Name: "Outro", Slug: "outro", Price: "10", Category: "maio",
		Variants: []store.VariantForm{{SKU: "OUT-M", Size: "m", Color: "Preto", Quantity: "0"}},
	})
	mustNoErr(t, err)
	upd := store.ProductForm{
		Name: "Biquíni Editado", Slug: "biquini-novo", Price: "150", Category: "biquini", Active: true,
		Variants: []store.VariantForm{
			{SKU: "BN-P", Size: "p", Color: "Azul", Threshold: "3"},
			{SKU: "BN-M", Size: "m", Color: "Azul", Threshold: "5"},
		},
		DeleteImageIDs: []string{edit.Images[0].ID.String()},
		LookProductIDs: []string{other.ID.String(), res.ID.String()},
		Images:         []store.Upload{pngUpload(), pngUpload()},
	}
	mustNoErr(t, store.UpdateProduct(ctx, pool, mem, res.ID, upd))
	edit, _ = store.LoadAdminProduct(ctx, pool, mem, res.ID)
	if edit.Name != "Biquíni Editado" || len(edit.Variants) != 2 || len(edit.Images) != 2 || !edit.LookIDs[other.ID] || edit.LookIDs[res.ID] {
		t.Fatalf("%+v", edit)
	}
	if err := store.UpdateProduct(ctx, pool, mem, newID(), upd); !errors.Is(err, store.ErrProductNotFound) {
		t.Fatal(err)
	}
	upd.Slug = "outro"
	if err := store.UpdateProduct(ctx, pool, mem, res.ID, upd); userMsg(t, err) != "Este slug já está em uso por outro produto." {
		t.Fatal(err)
	}
	dir, err := store.LoadAdminProducts(ctx, pool, "edit", "true", 1)
	mustNoErr(t, err)
	if len(dir.Products) != 1 || dir.Products[0].VariantCount != 2 || dir.Products[0].CategoryLabel() != "Biquíni" {
		t.Fatalf("%+v", dir.Products)
	}
	// Deleting a variant with orders is refused.
	vid := scalar[uuid.UUID](t, pool, `SELECT id FROM product_variants WHERE sku = 'OUT-M'`)
	exec(t, pool, `INSERT INTO orders (id, status) VALUES ($1, 'paid')`, newID())
	exec(t, pool, `INSERT INTO order_items (id, order_id, variant_id, product_name, variant_label, quantity, unit_price_cents)
		VALUES ($1, (SELECT id FROM orders LIMIT 1), $2, 'O', 'M', 1, 1000)`, newID(), vid)
	err = store.UpdateProduct(ctx, pool, mem, other.ID, store.ProductForm{Name: "Outro", Slug: "outro", Price: "10", Category: "maio", Active: true})
	if userMsg(t, err) != "Não é possível remover variante com pedidos vinculados." {
		t.Fatal(err)
	}
}

func TestAdminSetsAndBanners(t *testing.T) {
	pool := testutil.NewPool(t)
	mem := storage.NewMemory()
	res, err := store.CreateSet(ctx, pool, mem, store.SetForm{Name: "Verão", Slug: "verao", Price: "100", CompareAtPrice: "150", Active: true, Images: []store.Upload{pngUpload()}})
	mustNoErr(t, err)
	_, variantID, _ := seedVariant(t, pool, 5000, 2)
	edit, err := store.LoadAdminSet(ctx, pool, mem, res.ID)
	mustNoErr(t, err)
	if edit.PriceInput() != "100.00" || len(edit.Images) != 1 || len(edit.Slots) != 1 {
		t.Fatalf("%+v", edit)
	}
	if err := store.UpdateSet(ctx, pool, mem, res.ID, store.SetForm{
		Name: "Verão", Slug: "verao", Price: "100",
		Slots: []store.SetSlotForm{{Label: "Top", VariantID: "junk"}},
	}); userMsg(t, err) != "Variante inválida em uma das etapas." {
		t.Fatal(err)
	}
	mustNoErr(t, store.UpdateSet(ctx, pool, mem, res.ID, store.SetForm{
		Name: "Verão 2", Slug: "verao", Price: "100", Active: true,
		Slots: []store.SetSlotForm{{Label: "Top", VariantID: variantID.String()}, {}}, DeleteImageIDs: []string{edit.Images[0].ID.String()},
	}))
	edit, _ = store.LoadAdminSet(ctx, pool, mem, res.ID)
	if edit.Name != "Verão 2" || len(edit.Slots) != 2 || edit.Slots[0].VariantID != variantID.String() || len(edit.Images) != 0 {
		t.Fatalf("%+v", edit)
	}
	rows, _ := store.ListAdminSets(ctx, pool, mem)
	if len(rows) != 1 || rows[0].ImageCount != 0 {
		t.Fatal(rows)
	}
	mustNoErr(t, store.ToggleSet(ctx, pool, res.ID))
	mustNoErr(t, store.DeleteSet(ctx, pool, mem, res.ID))
	if n := scalar[int64](t, pool, `SELECT COUNT(*) FROM sets`); n != 0 {
		t.Fatal(n)
	}

	img := pngUpload()
	if err := store.CreateBanner(ctx, pool, mem, store.BannerForm{Title: " ", Image: &img}); userMsg(t, err) != "Informe o título do banner." {
		t.Fatal(err)
	}
	if err := store.CreateBanner(ctx, pool, storage.Unavailable{}, store.BannerForm{Title: "Verão", Image: &img}); userMsg(t, err) != store.MsgStorageUnavailable {
		t.Fatal(err)
	}
	mustNoErr(t, store.CreateBanner(ctx, pool, mem, store.BannerForm{Title: "Verão", LinkURL: "/produtos?sort=novos", SortOrder: "5000", Active: true, Image: &img}))
	slides, err := store.LoadHeroSlides(ctx, pool, mem)
	mustNoErr(t, err)
	if len(slides) != 1 || slides[0].CTALabel != "Ver coleção" || slides[0].LinkURL != "/produtos?sort=novos" {
		t.Fatalf("%+v", slides)
	}
	banners, _ := store.ListAdminBanners(ctx, pool, mem)
	if len(banners) != 1 || banners[0].SortOrder != 999 {
		t.Fatal(banners)
	}
	mustNoErr(t, store.ToggleBanner(ctx, pool, banners[0].ID))
	if s, _ := store.LoadHeroSlides(ctx, pool, mem); len(s) != 0 {
		t.Fatal("inactive banner")
	}
	mustNoErr(t, store.DeleteBanner(ctx, pool, mem, banners[0].ID))
}

func TestAdminOrderUpdateAndMPOverride(t *testing.T) {
	pool := testutil.NewPool(t)
	orderID, _ := seedOrderWithStock(t, pool, 2)
	actor := store.NamedActor("Admin")
	if _, err := store.AdminUpdateOrder(ctx, pool, orderID, store.AdminStatusInput{Status: "bogus"}, actor); userMsg(t, err) != "Status inválido." {
		t.Fatal(err)
	}
	if msg, err := store.AdminUpdateOrder(ctx, pool, orderID, store.AdminStatusInput{Status: "paid"}, actor); err != nil || msg != "Pedido atualizado com sucesso." {
		t.Fatal(msg, err)
	}
	_, err := store.AdminUpdateOrder(ctx, pool, orderID, store.AdminStatusInput{Status: "shipped", TrackingCode: "nope"}, actor)
	if !strings.Contains(store.AdminOrderErrorMessage(err), "Correios inválido") {
		t.Fatal(err)
	}
	mp := newID()
	exec(t, pool, `INSERT INTO orders (id, guest_email, status, total_cents, payment_provider) VALUES ($1, 'mp@example.com', 'pending_payment', 1000, 'mercadopago')`, mp)
	if _, err := store.AdminUpdateOrder(ctx, pool, mp, store.AdminStatusInput{Status: "paid"}, actor); userMsg(t, err) != store.MPManualBlockedMessage {
		t.Fatal(err)
	}
	reason := "Webhook do MP falhou; pagamento conferido no painel"
	code := store.OrderCode(mp)
	msg, err := store.AdminUpdateOrder(ctx, pool, mp, store.AdminStatusInput{Status: "paid", OverrideConfirm: &code, OverrideReason: &reason}, actor)
	if err != nil || msg != "Ação manual registrada no pedido." {
		t.Fatal(msg, err)
	}
	rows, err := pool.Query(ctx, `SELECT metadata->>'action', metadata->>'reason' FROM audit_events WHERE aggregate_id = $1 ORDER BY metadata->>'action' = 'mp_manual_override'`, mp.String())
	mustNoErr(t, err)
	var actions []string
	for rows.Next() {
		var a string
		var r *string
		mustNoErr(t, rows.Scan(&a, &r))
		actions = append(actions, a)
		if a == "mp_manual_override" && (r == nil || *r != reason) {
			t.Fatal("reason")
		}
	}
	rows.Close()
	if strings.Join(actions, ",") != "pay,mp_manual_override" {
		t.Fatal(actions)
	}
	if _, err := store.ApplyMPManualOverride(ctx, pool, mp, store.EventReopenPay, false, reason, actor); err == nil {
		t.Fatal("illegal override must fail")
	}
	if n := scalar[int64](t, pool, `SELECT count(*) FROM audit_events WHERE aggregate_id = $1`, mp.String()); n != 2 {
		t.Fatal("rolled back both rows")
	}
	detail, err := store.LoadAdminOrder(ctx, pool, mp)
	mustNoErr(t, err)
	if !detail.MercadoPago() || detail.Status != "paid" || !detail.Paid {
		t.Fatalf("%+v", detail)
	}
	if s, _, _, _ := detail.ManualOverride(); s != "refunded" {
		t.Fatal(s)
	}
	dir, err := store.LoadAdminOrders(ctx, pool, "mp@", "paid", 1)
	mustNoErr(t, err)
	if len(dir.Orders) != 1 || dir.Orders[0].ID != mp || store.ClampOrderStatus("x") != "all" {
		t.Fatal(dir.Orders)
	}
}

func TestLoginHookMergesGuestCartAndSetsCookie(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	_, variantID, _ := seedVariant(t, s.Pool, 1000, 5)
	guest, _, err := store.LoadOrCreateCart(ctx, s.Pool, nil, nil)
	mustNoErr(t, err)
	mustNoErr(t, store.AddItem(ctx, s.Pool, guest.ID, variantID, 2))
	c := s.Client(t)
	email := testutil.UniqueEmail("cart-merge")
	res := c.Do(http.MethodPost, "/signup", strings.NewReader(url.Values{"name": {"Ana"}, "email": {email}, "password": {"correct horse battery staple 9"}, "next": {"/"}}.Encode()),
		http.Header{"Content-Type": {"application/x-www-form-urlencoded"}, "Cookie": {testutil.DevCartCookie + "=" + guest.Token.String()}})
	if res.StatusCode != http.StatusSeeOther {
		t.Fatalf("%d %s", res.StatusCode, res.Body)
	}
	line, ok := testutil.FindSetCookie(res.SetCookies(), testutil.DevCartCookie)
	if !ok || !strings.Contains(line, guest.Token.String()) {
		t.Fatalf("cart cookie: %v", res.SetCookies())
	}
	owner := scalar[*uuid.UUID](t, s.Pool, `SELECT user_id FROM carts WHERE id = $1`, guest.ID)
	if owner == nil {
		t.Fatal("guest cart attached to the new account")
	}
}
