use topcoat::{runtime::Event, view::View};

pub struct CatalogFiltersProps {
  pub sort_value: String,
  pub selected_sizes: Vec<String>,
  pub selected_colors: Vec<String>,
  pub price_min: Option<String>,
  pub price_max: Option<String>,
  pub available_only: bool,
  pub sale_only: bool,
}

const SORT_OPTIONS: &[(&str, &str)] = &[
  ("destaque", "Em destaque"),
  ("relevantes", "Mais relevantes"),
  ("mais_vendidos", "Mais vendidos"),
  ("maior_desconto", "Maior desconto"),
  ("menor_preco", "Menor preço"),
  ("maior_preco", "Maior preço"),
  ("novos", "Mais novos"),
  ("antigos", "Mais antigos"),
];

const SIZES: &[(&str, &str)] = &[
  ("pp", "PP"),
  ("p", "P"),
  ("m", "M"),
  ("g", "G"),
  ("gg", "GG"),
];

// Simple sort dropdown component
pub fn sort_dropdown(sort_value: &str) -> impl View {
  let options: Vec<_> = SORT_OPTIONS.to_vec();

  topcoat::view::view! {
      <div class="flex items-center gap-2">
          <label for="sort-select" class="text-sm font-medium">
              "Ordenar por:"
          </label>
          <select
              id="sort-select"
              class="rounded border border-input bg-background px-3 py-2 text-sm"
              @change=$(|e: Event| {
                  let sort = e.target.value;
                  raw!(
                      r#"
                      const params = new URLSearchParams(window.location.search);
                      params.set("sort", String(${sort}));
                      params.set("page", "1");
                      history.replaceState(null, "", "?" + params.toString());
                      "#,
                      {
                          let _ = sort;
                      }
                  );
              })
          >
              for (value, label) in options {
                  let selected = sort_value == value;
                  <option value=(value) selected=(selected)>
                      (label)
                  </option>
              }
          </select>
      </div>
  }
}
