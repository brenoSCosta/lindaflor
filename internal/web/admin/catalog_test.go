package admin_test

import (
	"bytes"
	"net/http"
	"strings"
	"testing"

	"github.com/google/uuid"

	"github.com/wellington-vell/lindaflor/internal/httpx/toast"
	"github.com/wellington-vell/lindaflor/internal/testutil"
)

func productFields(slug string) map[string][]string {
	return map[string][]string{
		"name": {"Biquíni Novo"}, "slug": {slug}, "description": {"Lindo"}, "price": {"199,90"},
		"compare_at_price": {"249.90"}, "category": {"biquini"}, "featured": {"on"},
		"variant_sku[]": {"SKU-" + slug}, "variant_size[]": {"m"}, "variant_color[]": {"Coral"},
		"variant_color_hex[]": {"#ff7f50"}, "variant_quantity[]": {"7"},
	}
}

func TestProductCreateEditUpdate(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	a := adminAccount(t, s)
	slug := "biquini-novo-" + strings.ReplaceAll(uuid.NewString()[:8], "-", "")

	res := postMultipart(t, a.c, "/admin/produtos/create", productFields(slug),
		filePart{"images", "a.png", "image/png", pngBytes()})
	wantRedirect(t, res, "/admin/produtos/*")
	wantToast(t, a.c, toast.SuccessType, "Produto criado com sucesso.")
	id := uuid.MustParse(strings.TrimPrefix(res.Header.Get("Location"), "/admin/produtos/"))
	if n := scalar[int64](t, s, `SELECT price_in_cents FROM products WHERE id = $1`, id); n != 19990 {
		t.Fatalf("price = %d", n)
	}
	if n := scalar[int32](t, s, `SELECT i.quantity FROM inventory i JOIN product_variants v ON v.id = i.variant_id WHERE v.product_id = $1`, id); n != 7 {
		t.Fatalf("stock = %d", n)
	}
	if n := scalar[int64](t, s, `SELECT COUNT(*) FROM product_images WHERE product_id = $1`, id); n != 1 {
		t.Fatalf("images = %d", n)
	}

	// Edit page shows the stored values, the image and the variant.
	other, _, _ := seedProduct(t, s, "Saída Combina", 1)
	page := a.c.Get("/admin/produtos/" + id.String())
	wantStatus(t, page, http.StatusOK)
	wantContains(t, page.Body, "Editar produto", `value="199.90"`, `value="249.90"`, "SKU-"+slug, "#FF7F50",
		`name="delete_image_id"`, "Complete o look", "Saída Combina — Biquíni")

	// Validation errors toast and go back to the form.
	bad := productFields("x")
	bad["price"] = []string{"abc"}
	wantRedirect(t, postMultipart(t, a.c, "/admin/produtos/create", bad), "/admin/produtos/novo")
	wantToast(t, a.c, toast.ErrorType, "Preço inválido. Use 199.90.")
	noVariant := productFields("y-" + slug)
	delete(noVariant, "variant_sku[]")
	wantRedirect(t, postMultipart(t, a.c, "/admin/produtos/create", noVariant), "/admin/produtos/novo")
	wantToast(t, a.c, toast.ErrorType, "Informe ao menos uma variante completa.")
	wantRedirect(t, postMultipart(t, a.c, "/admin/produtos/create", productFields(slug)), "/admin/produtos/novo")
	wantToast(t, a.c, toast.ErrorType, "Este slug já está em uso por outro produto.")
	wantRedirect(t, postMultipart(t, a.c, "/admin/produtos/create", productFields("z-"+slug),
		filePart{"images", "a.gif", "image/gif", []byte("GIF89a")}), "/admin/produtos/novo")
	if q := flash(t, a.c); q[len(q)-1].Type != toast.ErrorType {
		t.Fatalf("bad image accepted: %+v", q)
	}

	// Update: rename, new variant SKU replaces the old one, a look, inactive.
	imageID := scalar[uuid.UUID](t, s, `SELECT id FROM product_images WHERE product_id = $1`, id)
	upd := map[string][]string{
		"product_id": {id.String()}, "name": {"Biquíni Editado"}, "slug": {slug}, "price": {"150"},
		"category": {"maio"}, "variant_sku[]": {"NOVO-" + slug}, "variant_size[]": {"g"},
		"variant_color[]": {"Azul"}, "variant_color_hex[]": {""}, "variant_threshold[]": {"3"},
		"look_product_ids[]": {other.String()}, "delete_image_id": {imageID.String()},
	}
	res = postMultipart(t, a.c, "/admin/produtos/update", upd)
	wantRedirect(t, res, "/admin/produtos/"+id.String())
	wantToast(t, a.c, toast.SuccessType, "Alterações salvas com sucesso.")
	if active := scalar[bool](t, s, `SELECT active FROM products WHERE id = $1`, id); active {
		t.Fatal("unchecked active must deactivate")
	}
	if sku := scalar[string](t, s, `SELECT sku FROM product_variants WHERE product_id = $1`, id); sku != "NOVO-"+slug {
		t.Fatalf("sku = %q", sku)
	}
	if n := scalar[int64](t, s, `SELECT COUNT(*) FROM product_looks WHERE product_id = $1 AND related_product_id = $2`, id, other); n != 1 {
		t.Fatal("look not saved")
	}
	if n := scalar[int64](t, s, `SELECT COUNT(*) FROM product_images WHERE product_id = $1`, id); n != 0 {
		t.Fatal("image not deleted")
	}

	// Regression: the create form says "add more variants by editing the
	// product", but the edit page had no row for a new one. Its blank row
	// is ignored when empty and adds a variant (with an inventory row) when
	// filled.
	page = a.c.Get("/admin/produtos/" + id.String())
	wantContains(t, page.Body, "SKU (nova variante)")
	if n := strings.Count(page.Body, `name="variant_sku[]"`); n != 2 {
		t.Fatalf("variant rows = %d, want the stored one + a blank one", n)
	}
	upd["variant_sku[]"] = []string{"NOVO-" + slug, ""}
	upd["variant_size[]"] = []string{"g", ""}
	upd["variant_color[]"] = []string{"Azul", ""}
	upd["variant_color_hex[]"] = []string{"", ""}
	upd["variant_threshold[]"] = []string{"3", "5"}
	wantRedirect(t, postMultipart(t, a.c, "/admin/produtos/update", upd), "/admin/produtos/"+id.String())
	if n := scalar[int64](t, s, `SELECT COUNT(*) FROM product_variants WHERE product_id = $1`, id); n != 1 {
		t.Fatalf("blank row created a variant: %d", n)
	}
	upd["variant_sku[]"] = []string{"NOVO-" + slug, "EXTRA-" + slug}
	upd["variant_size[]"] = []string{"g", "m"}
	upd["variant_color[]"] = []string{"Azul", "Coral"}
	wantRedirect(t, postMultipart(t, a.c, "/admin/produtos/update", upd), "/admin/produtos/"+id.String())
	wantToast(t, a.c, toast.SuccessType, "Alterações salvas com sucesso.")
	if n := scalar[int64](t, s, `SELECT COUNT(*) FROM product_variants v JOIN inventory i ON i.variant_id = v.id WHERE v.product_id = $1`, id); n != 2 {
		t.Fatalf("variants with inventory = %d, want 2", n)
	}
	upd["variant_sku[]"] = []string{"NOVO-" + slug}
	upd["variant_size[]"] = []string{"g"}
	upd["variant_color[]"] = []string{"Azul"}
	upd["variant_color_hex[]"] = []string{""}
	upd["variant_threshold[]"] = []string{"3"}

	// Unknown / invalid product ids go back to the list.
	upd["product_id"] = []string{uuid.NewString()}
	wantRedirect(t, postMultipart(t, a.c, "/admin/produtos/update", upd), "/admin/produtos")
	wantToast(t, a.c, toast.ErrorType, "Produto não encontrado.")
	upd["product_id"] = []string{"nope"}
	wantRedirect(t, postMultipart(t, a.c, "/admin/produtos/update", upd), "/admin/produtos")
	wantToast(t, a.c, toast.ErrorType, "Produto inválido.")
	upd["product_id"] = []string{id.String()}
	upd["variant_size[]"] = nil
	wantRedirect(t, postMultipart(t, a.c, "/admin/produtos/update", upd), "/admin/produtos/"+id.String())
	wantToast(t, a.c, toast.ErrorType, "Variantes inconsistentes. Confira os campos.")
}

func TestProductImageLimits(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	a := adminAccount(t, s)
	files := make([]filePart, 9)
	for i := range files {
		files[i] = filePart{"images", "a.png", "image/png", pngBytes()}
	}
	wantRedirect(t, postMultipart(t, a.c, "/admin/produtos/create", productFields("nove-imagens"), files...), "/admin/produtos/novo")
	wantToast(t, a.c, toast.ErrorType, "O produto pode ter no máximo 8 imagens")

	// Regression: delete_image_id values that are not this product's images
	// were still subtracted from the count, so a stale or made-up id let a
	// full product take a 9th image (and a stale one failed the update).
	res0 := postMultipart(t, a.c, "/admin/produtos/create", productFields("oito-imagens"), files[:8]...)
	wantRedirect(t, res0, "/admin/produtos/*")
	full := uuid.MustParse(strings.TrimPrefix(res0.Header.Get("Location"), "/admin/produtos/"))
	upd := map[string][]string{
		"product_id": {full.String()}, "name": {"Oito"}, "slug": {"oito-imagens"}, "price": {"199.90"},
		"category": {"biquini"}, "variant_sku[]": {"SKU-oito-imagens"}, "variant_size[]": {"m"},
		"variant_color[]": {"Coral"}, "variant_color_hex[]": {""}, "variant_threshold[]": {"5"},
		"delete_image_id": {uuid.NewString()},
	}
	wantRedirect(t, postMultipart(t, a.c, "/admin/produtos/update", upd, files[0]), "/admin/produtos/"+full.String())
	wantToast(t, a.c, toast.ErrorType, "O produto pode ter no máximo 8 imagens")
	if n := scalar[int64](t, s, `SELECT COUNT(*) FROM product_images WHERE product_id = $1`, full); n != 8 {
		t.Fatalf("images = %d, want 8", n)
	}

	// 20 MiB route cap: a ~3 MiB body passes the limit (the image is then
	// refused by size), a 21 MiB body is a 413.
	big := append(pngBytes(), bytes.Repeat([]byte{0}, 3<<20)...)
	res := postMultipart(t, a.c, "/admin/produtos/create", productFields("grande"), filePart{"images", "a.png", "image/png", big})
	wantRedirect(t, res, "/admin/produtos/novo")
	huge := append(pngBytes(), bytes.Repeat([]byte{0}, 21<<20)...)
	res = postMultipart(t, a.c, "/admin/produtos/create", productFields("enorme"), filePart{"images", "a.png", "image/png", huge})
	wantStatus(t, res, http.StatusRequestEntityTooLarge)
	res = postMultipart(t, a.c, "/admin/produtos/update", map[string][]string{"product_id": {uuid.NewString()}},
		filePart{"images", "a.png", "image/png", huge})
	wantStatus(t, res, http.StatusRequestEntityTooLarge)
	// Other admin forms keep the 1 MiB global cap.
	res = a.c.PostForm("/admin/cupons/create", form("code", strings.Repeat("A", 2<<20)))
	wantStatus(t, res, http.StatusRequestEntityTooLarge)
}

func TestProductDirectoryFragment(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	a := adminAccount(t, s)
	seedProduct(t, s, "Maiô Listrado", 3)
	seedProduct(t, s, "Biquíni Floral", 4)
	exec(t, s, `UPDATE products SET active = false WHERE name = 'Maiô Listrado'`)

	res := a.c.Get("/admin/produtos?q=Floral")
	wantStatus(t, res, http.StatusOK)
	wantContains(t, res.Body, "Biquíni Floral", `hx-get="/admin/produtos/lista"`, `hx-trigger="input changed delay:300ms from:#product-search`)
	wantNotContains(t, res.Body, "Maiô Listrado")

	frag := a.c.HTMX().Get("/admin/produtos/lista?q=Mai%C3%B4&active=false")
	wantStatus(t, frag, http.StatusOK)
	wantContains(t, frag.Body, `id="product-results"`, "Maiô Listrado", "Inativo", `name="active" value="false"`)
	wantNotContains(t, frag.Body, "<html", "Biquíni Floral")
	if got := frag.Header.Get("HX-Replace-Url"); got != "/admin/produtos?q=Mai%C3%B4&active=false" {
		t.Fatalf("HX-Replace-Url = %q", got)
	}
	// Literal search: % is not a wildcard.
	wantContains(t, a.c.Get("/admin/produtos/lista?q=%25").Body, "Nenhum produto encontrado.")
}

func TestSetsCreateToggleUpdateDelete(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	a := adminAccount(t, s)
	product, variant, _ := seedProduct(t, s, "Top Conjunto", 5)

	res := postMultipart(t, a.c, "/admin/sets/create", map[string][]string{
		"name": {"Conjunto Verão"}, "slug": {"conjunto-verao"}, "price": {"299.90"}, "active": {"on"},
	}, filePart{"images", "c.png", "image/png", pngBytes()})
	wantRedirect(t, res, "/admin/sets/*")
	wantToast(t, a.c, toast.SuccessType, "Conjunto criado com sucesso.")
	id := strings.TrimPrefix(res.Header.Get("Location"), "/admin/sets/")

	list := a.c.Get("/admin/sets")
	wantContains(t, list.Body, "Conjunto Verão", "conjunto-verao", "R$ 299", "Desativar")

	edit := a.c.Get("/admin/sets/" + id)
	wantContains(t, edit.Body, "Editar conjunto", "Top Conjunto", "Etapas do bundle", "Selecione…", `value="299.90"`)

	res = postMultipart(t, a.c, "/admin/sets/"+id+"/update", map[string][]string{
		"name": {"Conjunto Verão"}, "slug": {"conjunto-verao"}, "price": {"280"}, "active": {"on"},
		"member_product_ids[]": {product.String()},
		"slot_label[]":         {"Parte de cima", ""}, "slot_variant[]": {variant.String(), ""},
	})
	wantRedirect(t, res, "/admin/sets/"+id)
	wantToast(t, a.c, toast.SuccessType, "Alterações salvas com sucesso.")
	if n := scalar[int64](t, s, `SELECT COUNT(*) FROM set_slots WHERE set_id = $1`, id); n != 1 {
		t.Fatalf("slots = %d", n)
	}
	res = postMultipart(t, a.c, "/admin/sets/"+id+"/update", map[string][]string{
		"name": {"Conjunto Verão"}, "slug": {"conjunto-verao"}, "slot_label[]": {"X"}, "slot_variant[]": {"nope"},
	})
	wantRedirect(t, res, "/admin/sets/"+id)
	wantToast(t, a.c, toast.ErrorType, "Variante inválida em uma das etapas.")
	wantRedirect(t, postMultipart(t, a.c, "/admin/sets/"+uuid.NewString()+"/update", map[string][]string{"name": {"x"}}), "/admin/sets")
	wantToast(t, a.c, toast.ErrorType, "Conjunto não encontrado.")

	wantRedirect(t, a.c.PostForm("/admin/sets/toggle", form("set_id", id)), "/admin/sets")
	wantToast(t, a.c, toast.SuccessType, "Conjunto atualizado.")
	if scalar[bool](t, s, `SELECT active FROM sets WHERE id = $1`, id) {
		t.Fatal("toggle did not deactivate")
	}
	wantRedirect(t, a.c.PostForm("/admin/sets/toggle", form("set_id", "x")), "/admin/sets")
	wantToast(t, a.c, toast.ErrorType, "Conjunto inválido.")

	// htmx: X-Toast + HX-Location, never a 303.
	h := s.Client(t)
	h.HTTP.Jar = a.c.HTTP.Jar
	res = h.HTMX().PostForm("/admin/sets/delete", form("set_id", id))
	wantStatus(t, res, http.StatusOK)
	if res.Header.Get("HX-Location") != "/admin/sets" || !strings.Contains(res.Header.Get("X-Toast"), "Conjunto%20exclu") {
		t.Fatalf("htmx delete headers: %v", res.Header)
	}
	if n := scalar[int64](t, s, `SELECT COUNT(*) FROM sets WHERE id = $1`, id); n != 0 {
		t.Fatal("set not deleted")
	}
}

func TestBannersCreateToggleDelete(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	a := adminAccount(t, s)
	fields := map[string][]string{"title": {"Verão 2026"}, "subtitle": {"Nova coleção"}, "link_url": {"/produtos?sort=novos"}, "sort_order": {"2"}, "active": {"on"}}

	wantRedirect(t, postMultipart(t, a.c, "/admin/banners/create", fields), "/admin/banners")
	wantToast(t, a.c, toast.ErrorType, "Envie uma imagem para o banner.")
	bad := map[string][]string{"title": {"x"}, "link_url": {"https://evil.example"}}
	wantRedirect(t, postMultipart(t, a.c, "/admin/banners/create", bad, filePart{"images", "b.png", "image/png", pngBytes()}), "/admin/banners")
	wantToast(t, a.c, toast.ErrorType, "O link deve ser um caminho interno, começando com /.")

	wantRedirect(t, postMultipart(t, a.c, "/admin/banners/create", fields, filePart{"images", "b.png", "image/png", pngBytes()}), "/admin/banners")
	wantToast(t, a.c, toast.SuccessType, "Banner criado.")
	id := scalar[uuid.UUID](t, s, `SELECT id FROM home_banners LIMIT 1`)
	page := a.c.Get("/admin/banners")
	wantContains(t, page.Body, "Verão 2026", "Nova coleção", "/produtos?sort=novos", "Ativo", "Desativar")

	wantRedirect(t, a.c.PostForm("/admin/banners/toggle", form("banner_id", id.String())), "/admin/banners")
	wantToast(t, a.c, toast.SuccessType, "Banner atualizado.")
	wantRedirect(t, a.c.PostForm("/admin/banners/delete", form("banner_id", "bad")), "/admin/banners")
	wantToast(t, a.c, toast.ErrorType, "Banner inválido.")
	wantRedirect(t, a.c.PostForm("/admin/banners/delete", form("banner_id", id.String())), "/admin/banners")
	wantToast(t, a.c, toast.SuccessType, "Banner excluído.")
	if n := scalar[int64](t, s, `SELECT COUNT(*) FROM home_banners`); n != 0 {
		t.Fatal("banner not deleted")
	}
}

func TestStockSearchAndAdjust(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	a := adminAccount(t, s)
	_, _, inv := seedProduct(t, s, "Biquíni Estoque", 5)
	seedProduct(t, s, "Outra Peça", 1)

	frag := a.c.HTMX().Get("/admin/estoque/lista?q=Estoque")
	wantStatus(t, frag, http.StatusOK)
	wantContains(t, frag.Body, `id="stock-results"`, "Biquíni Estoque", "Depósito Principal", `value="Estoque"`)
	wantNotContains(t, frag.Body, "Outra Peça")
	a.c.Headers = http.Header{}

	res := a.c.PostForm("/admin/estoque", form("inventory_id", inv.String(), "delta", "-2", "reason", "Avaria", "q", "Estoque"))
	wantRedirect(t, res, "/admin/estoque?q=Estoque")
	wantToast(t, a.c, toast.SuccessType, "Estoque ajustado.")
	if n := scalar[int32](t, s, `SELECT quantity FROM inventory WHERE id = $1`, inv); n != 3 {
		t.Fatalf("quantity = %d", n)
	}
	if n := scalar[int64](t, s, `SELECT COUNT(*) FROM audit_events WHERE aggregate_type = 'inventory' AND aggregate_id = $1 AND metadata->>'reason' = 'Avaria'`, inv.String()); n != 1 {
		t.Fatal("adjust not audited")
	}
	wantRedirect(t, a.c.PostForm("/admin/estoque", form("inventory_id", inv.String(), "delta", "-10", "reason", "x")), "/admin/estoque")
	if q := flash(t, a.c); q[len(q)-1].Type != toast.ErrorType {
		t.Fatalf("over-withdrawal accepted: %+v", q)
	}
	wantRedirect(t, a.c.PostForm("/admin/estoque", form("inventory_id", inv.String(), "delta", "1", "reason", "")), "/admin/estoque")
	wantToast(t, a.c, toast.ErrorType, "informe um motivo (até 200 caracteres)")
	wantRedirect(t, a.c.PostForm("/admin/estoque", form("inventory_id", "x", "delta", "1", "reason", "r")), "/admin/estoque")
	wantToast(t, a.c, toast.ErrorType, "inventory id inválido")
}
