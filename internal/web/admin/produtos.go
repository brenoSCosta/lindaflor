package admin

import (
	"errors"
	"net/http"

	"github.com/go-chi/chi/v5"

	"github.com/wellington-vell/lindaflor/internal/httpx"
	"github.com/wellington-vell/lindaflor/internal/store"
	"github.com/wellington-vell/lindaflor/internal/web/render"
)

// /admin/produtos (src/app/admin/produtos*.rs).

const (
	productsPath     = "/admin/produtos"
	productsListPath = "/admin/produtos/lista"
)

func productsURL(path, q, active string, page int64) string {
	if active == "all" {
		active = ""
	}
	return withQuery(path, "q", q, "active", active, "page", pageParam(page))
}

type productDirectory struct {
	store.AdminProductDirectory
	Filters []dirLink
	Pager   pagerData
}

func (h *handler) loadProducts(r *http.Request, q, active string, page int64) (productDirectory, error) {
	d, err := store.LoadAdminProducts(r.Context(), h.d.Pool, q, active, store.ClampPage(page))
	if err != nil {
		return productDirectory{}, err
	}
	out := productDirectory{AdminProductDirectory: d}
	for _, f := range store.ProductActiveFilters {
		out.Filters = append(out.Filters, dirLink{
			Label: f[1], Selected: d.Active == f[0],
			Page: productsURL(productsPath, d.Query, f[0], 1), Frag: productsURL(productsListPath, d.Query, f[0], 1),
		})
	}
	out.Pager = pagerFor(d.Pager.Page, d.Pager.TotalPages, func(p int64) dirLink {
		return dirLink{Page: productsURL(productsPath, d.Query, d.Active, p), Frag: productsURL(productsListPath, d.Query, d.Active, p)}
	})
	return out, nil
}

// GET /admin/produtos.
func (h *handler) productsPage(w http.ResponseWriter, r *http.Request) {
	if _, ok := h.actor(w, r); !ok {
		return
	}
	q := r.URL.Query()
	d, err := h.loadProducts(r, q.Get("q"), q.Get("active"), store.ParsePage(q.Get("page")))
	if err != nil {
		serverError(w, r, err)
		return
	}
	head(r, "Produtos")
	render.Page(w, r, h.d, productsPage(d))
}

// GET /admin/produtos/lista (htmx fragment).
func (h *handler) productsList(w http.ResponseWriter, r *http.Request) {
	if !httpx.IsHTMX(r) {
		h.productsPage(w, r)
		return
	}
	if _, ok := h.actor(w, r); !ok {
		return
	}
	q := r.URL.Query()
	d, err := h.loadProducts(r, q.Get("q"), q.Get("active"), store.ParsePage(q.Get("page")))
	if err != nil {
		serverError(w, r, err)
		return
	}
	fragment(w, r, productsURL(productsPath, d.Query, d.Active, d.AdminProductDirectory.Pager.Page), productResults(d))
}

// GET /admin/produtos/novo.
func (h *handler) productNewPage(w http.ResponseWriter, r *http.Request) {
	if _, ok := h.actor(w, r); !ok {
		return
	}
	head(r, "Novo produto")
	render.Page(w, r, h.d, productNewPage())
}

func productForm(f multipartForm, edit bool) (store.ProductForm, error) {
	pf := store.ProductForm{
		ProductID: f.first("product_id"), Name: f.first("name"), Slug: f.first("slug"),
		Description: f.first("description"), Price: f.first("price"), CompareAtPrice: f.first("compare_at_price"),
		Category: f.first("category"), Featured: f.has("featured"), Active: f.has("active"),
		DeleteImageIDs: f.all("delete_image_id"), LookProductIDs: f.all("look_product_ids[]"),
		Images: f.files,
	}
	variants, err := store.ZipVariantForms(f.all("variant_sku[]"), f.all("variant_size[]"), f.all("variant_color[]"),
		f.all("variant_color_hex[]"), f.all("variant_quantity[]"), f.all("variant_threshold[]"), edit)
	pf.Variants = variants
	return pf, err
}

// productBaseError reports the first base-field error, so a broken name
// or price is reported before a variant problem (Rust validation order).
func productBaseError(pf store.ProductForm) error {
	if _, err := store.ValidateProductName(pf.Name); err != nil {
		return err
	}
	if _, err := store.ValidateProductSlug(pf.Slug); err != nil {
		return err
	}
	price, err := store.ParseProductPrice(pf.Price)
	if err != nil {
		return err
	}
	if _, err := store.ParseCompareAtPrice(pf.CompareAtPrice, price); err != nil {
		return err
	}
	_, err = store.ValidateCategory(pf.Category)
	return err
}

// POST /admin/produtos/create (multipart, 20 MiB).
func (h *handler) productCreate(w http.ResponseWriter, r *http.Request) {
	if _, ok := h.actor(w, r); !ok {
		return
	}
	f, ok := parseMultipart(w, r)
	if !ok {
		return
	}
	back := "/admin/produtos/novo"
	pf, zipErr := productForm(f, false)
	if err := productBaseError(pf); err != nil {
		failTo(w, r, back, userMessage(err, msgGenericError))
		return
	}
	if zipErr != nil {
		failTo(w, r, back, userMessage(zipErr, msgGenericError))
		return
	}
	res, err := store.CreateProduct(r.Context(), h.d.Pool, h.d.Storage, pf)
	if err != nil {
		if msg, ok := store.UserMessage(err); ok {
			failTo(w, r, back, msg)
			return
		}
		serverError(w, r, err)
		return
	}
	edit := "/admin/produtos/" + res.ID.String()
	if res.ImageFailed {
		failTo(w, r, edit, res.Message())
		return
	}
	okTo(w, r, edit, res.Message())
}

// GET /admin/produtos/{id}.
func (h *handler) productEditPage(w http.ResponseWriter, r *http.Request) {
	if _, ok := h.actor(w, r); !ok {
		return
	}
	var p *store.AdminProductEdit
	if id, ok := parseUUID(chi.URLParam(r, "id")); ok {
		var err error
		if p, err = store.LoadAdminProduct(r.Context(), h.d.Pool, h.d.Storage, id); err != nil {
			serverError(w, r, err)
			return
		}
	}
	if p == nil {
		head(r, "Produto não encontrado")
		render.Page(w, r, h.d, notFoundBody("Produto não encontrado", productsPath, "Voltar para produtos"))
		return
	}
	head(r, "Editar produto")
	render.Page(w, r, h.d, productEditPage(p))
}

// POST /admin/produtos/update (multipart, 20 MiB).
func (h *handler) productUpdate(w http.ResponseWriter, r *http.Request) {
	if _, ok := h.actor(w, r); !ok {
		return
	}
	f, ok := parseMultipart(w, r)
	if !ok {
		return
	}
	id, ok := parseUUID(f.first("product_id"))
	if !ok {
		failTo(w, r, productsPath, "Produto inválido.")
		return
	}
	back := "/admin/produtos/" + id.String()
	pf, zipErr := productForm(f, true)
	if zipErr != nil {
		// The Rust handler checks existence and base fields first.
		if err := productBaseError(pf); err != nil {
			zipErr = err
		}
		failTo(w, r, back, userMessage(zipErr, msgGenericError))
		return
	}
	err := store.UpdateProduct(r.Context(), h.d.Pool, h.d.Storage, id, pf)
	switch {
	case err == nil:
		okTo(w, r, back, "Alterações salvas com sucesso.")
	case errors.Is(err, store.ErrProductNotFound):
		failTo(w, r, productsPath, "Produto não encontrado.")
	default:
		if msg, ok := store.UserMessage(err); ok {
			failTo(w, r, back, msg)
			return
		}
		serverError(w, r, err)
	}
}
