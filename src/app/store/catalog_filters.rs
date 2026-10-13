//! Catalog sort select, "Filtros" panel and active-filter chips.
//!
//! Everything is one plain GET form: submitting reloads the page with query
//! params that the catalog/set pages already parse and normalize.

use topcoat::{
  Result,
  view::{View, attributes, component, view},
};

use crate::app::store::queries::size_label;
use crate::components::button::{ButtonSize, ButtonVariant, button_variants};
use crate::components::checkbox::checkbox;
use crate::components::input::input;

pub const SORT_OPTIONS: [(&str, &str); 8] = [
  ("destaque", "Em destaque"),
  ("relevantes", "Mais relevantes"),
  ("mais_vendidos", "Mais vendidos"),
  ("maior_desconto", "Maior desconto"),
  ("menor_preco", "Menor preço"),
  ("maior_preco", "Maior preço"),
  ("novos", "Mais novos"),
  ("antigos", "Mais antigos"),
];

const SIZES: [&str; 5] = ["pp", "p", "m", "g", "gg"];

/// Current filter values; `href` builds links that keep them.
#[derive(Clone)]
struct FilterState {
  base: String,
  category: String,
  search: String,
  sort: String,
  sizes: Vec<String>,
  colors: Vec<String>,
  preco_min: Option<String>,
  preco_max: Option<String>,
  disponivel: bool,
  sale: bool,
}

/// Which filter a link drops.
enum Drop<'a> {
  Size(&'a str),
  Color(&'a str),
  Price,
  Disponivel,
  Sale,
  AllFacets,
}

impl FilterState {
  fn href(&self, drop: Drop<'_>) -> String {
    let mut pairs: Vec<(&str, String)> = Vec::new();
    if !self.category.is_empty() && self.category != "all" {
      pairs.push(("category", self.category.clone()));
    }
    if !self.search.is_empty() {
      pairs.push(("search", self.search.clone()));
    }
    if !self.sort.is_empty() && self.sort != "destaque" {
      pairs.push(("sort", self.sort.clone()));
    }
    if !matches!(drop, Drop::AllFacets) {
      for size in &self.sizes {
        if !matches!(drop, Drop::Size(s) if s == size) {
          pairs.push(("tamanho", size.clone()));
        }
      }
      for color in &self.colors {
        if !matches!(drop, Drop::Color(c) if c == color) {
          pairs.push(("cor", color.clone()));
        }
      }
      if !matches!(drop, Drop::Price) {
        if let Some(min) = &self.preco_min {
          pairs.push(("preco_min", min.clone()));
        }
        if let Some(max) = &self.preco_max {
          pairs.push(("preco_max", max.clone()));
        }
      }
      if self.disponivel && !matches!(drop, Drop::Disponivel) {
        pairs.push(("disponivel", "true".into()));
      }
      if self.sale && !matches!(drop, Drop::Sale) {
        pairs.push(("sale", "true".into()));
      }
    }
    filter_href(&self.base, &pairs)
  }
}

/// Every decoded `key=value` pair of a raw query string, in order.
/// `#[query_params]` can't deserialize sequences, so the catalog reads its
/// repeated facets (`?tamanho=p&tamanho=m`) from these; callers still
/// normalize the values.
pub fn query_pairs(query: &str) -> Vec<(String, String)> {
  let Ok(url) = reqwest::Url::parse(&format!("http://localhost/?{query}"))
  else {
    return Vec::new();
  };
  url
    .query_pairs()
    .map(|(k, v)| (k.into_owned(), v.into_owned()))
    .collect()
}

/// Every non-empty value of `key` in `pairs`.
pub fn repeated_values(pairs: &[(String, String)], key: &str) -> Vec<String> {
  pairs
    .iter()
    .filter(|(k, v)| k == key && !v.is_empty())
    .map(|(_, v)| v.clone())
    .collect()
}

/// `base` path plus properly percent-encoded query pairs.
fn filter_href(base: &str, pairs: &[(&str, String)]) -> String {
  let mut url =
    reqwest::Url::parse("http://localhost/").expect("static base URL");
  url.set_path(base);
  if !pairs.is_empty() {
    url
      .query_pairs_mut()
      .extend_pairs(pairs.iter().map(|(k, v)| (*k, v.as_str())));
  }
  match url.query() {
    Some(query) => format!("{}?{query}", url.path()),
    None => url.path().to_string(),
  }
}

const CHIP_CLASS: &str = "inline-flex items-center gap-1 rounded-full bg-secondary px-3 py-1 text-sm hover:opacity-80";

/// Sort + filters toolbar. `form_action` is the page path the form submits
/// to (catalog or a set), so chips and "Limpar" stay on that page.
#[component]
pub async fn catalog_toolbar(
  category: String,
  search: String,
  sort: String,
  tamanho: Vec<String>,
  cor: Vec<String>,
  preco_min: Option<String>,
  preco_max: Option<String>,
  disponivel: bool,
  sale: bool,
  available_colors: Vec<(String, Option<String>)>,
  #[into]
  #[default("/produtos".to_string())]
  form_action: String,
) -> Result<impl View> {
  let state = FilterState {
    base: form_action.clone(),
    category: category.clone(),
    search: search.clone(),
    sort: sort.clone(),
    sizes: tamanho.clone(),
    colors: cor.clone(),
    preco_min: preco_min.clone(),
    preco_max: preco_max.clone(),
    disponivel,
    sale,
  };
  let has_filters = !tamanho.is_empty()
    || !cor.is_empty()
    || preco_min.is_some()
    || preco_max.is_some()
    || disponivel
    || sale;
  let clear_url = state.href(Drop::AllFacets);
  let price_label = match (&preco_min, &preco_max) {
    (Some(min), Some(max)) => Some(format!("R$ {min} – R$ {max}")),
    (Some(min), None) => Some(format!("A partir de R$ {min}")),
    (None, Some(max)) => Some(format!("Até R$ {max}")),
    (None, None) => None,
  };
  let size_chips: Vec<(String, String)> = tamanho
    .iter()
    .map(|size| (size_label(size).to_string(), state.href(Drop::Size(size))))
    .collect();
  let color_chips: Vec<(String, String)> = cor
    .iter()
    .map(|color| (color.clone(), state.href(Drop::Color(color))))
    .collect();
  let price_chip = price_label.map(|label| (label, state.href(Drop::Price)));
  let disponivel_chip = disponivel.then(|| state.href(Drop::Disponivel));
  let sale_chip = sale.then(|| state.href(Drop::Sale));
  let color_options: Vec<(usize, String, Option<String>, bool)> =
    available_colors
      .into_iter()
      .enumerate()
      .map(|(index, (name, hex))| {
        let selected = cor.contains(&name);
        (index, name, hex, selected)
      })
      .collect();
  let open_panel = has_filters;

  Ok(view! {
      <form method="get" action=(form_action) class="space-y-4">
          if !category.is_empty() && category != "all" {
              <input type="hidden" name="category" value=(category.clone())>
          }
          if !search.is_empty() {
              <input type="hidden" name="search" value=(search.clone())>
          }

          <div class="flex flex-wrap items-center justify-end gap-3">
              <label for="sort-select" class="text-sm font-medium whitespace-nowrap">"Ordenar:"</label>
              <select
                  id="sort-select"
                  name="sort"
                  onchange="this.form.requestSubmit()"
                  class="rounded border border-input bg-background px-3 py-2 text-sm"
              >
                  for (value, label) in SORT_OPTIONS {
                      <option value=(value) selected=(sort == value)>(label)</option>
                  }
              </select>
          </div>

          if has_filters {
              <div class="flex flex-wrap items-center gap-2">
                  #[key(href.clone())]
                  for (label, href) in size_chips {
                      <a href=(href) class=(CHIP_CLASS)>(label) <span aria-hidden="true">"×"</span></a>
                  }
                  #[key(href.clone())]
                  for (label, href) in color_chips {
                      <a href=(href) class=(CHIP_CLASS)>(label) <span aria-hidden="true">"×"</span></a>
                  }
                  if let Some((label, href)) = price_chip {
                      <a href=(href) class=(CHIP_CLASS)>(label) <span aria-hidden="true">"×"</span></a>
                  }
                  if let Some(href) = disponivel_chip {
                      <a href=(href) class=(CHIP_CLASS)>"Disponíveis" <span aria-hidden="true">"×"</span></a>
                  }
                  if let Some(href) = sale_chip {
                      <a href=(href) class=(CHIP_CLASS)>"Em promoção" <span aria-hidden="true">"×"</span></a>
                  }
                  <a href=(clear_url.clone()) class="text-sm text-muted-foreground underline">"Limpar tudo"</a>
              </div>
          }

          <details class="rounded-lg border border-border p-4" open=(open_panel)>
              <summary class="cursor-pointer font-medium">"Filtrar"</summary>
              <div class="mt-4 grid gap-6 md:grid-cols-4">
                  <fieldset>
                      <legend class="mb-2 text-sm font-semibold">"Tamanhos"</legend>
                      <div class="space-y-2">
                          for size in SIZES {
                              let id = format!("size-{size}");
                              <div class="flex items-center gap-2">
                                  checkbox(attrs: attributes! {
                                      id=(id.clone())
                                      name="tamanho"
                                      value=(size)
                                      checked=(tamanho.iter().any(|s| s == size))
                                  })
                                  <label for=(id) class="cursor-pointer text-sm">(size_label(size))</label>
                              </div>
                          }
                      </div>
                  </fieldset>

                  if !color_options.is_empty() {
                      <fieldset>
                          <legend class="mb-2 text-sm font-semibold">"Cores"</legend>
                          <div class="max-h-56 space-y-2 overflow-y-auto">
                              #[key(index.to_string())]
                              for (index, name, hex, selected) in color_options {
                                  let id = format!("color-{index}");
                                  <div class="flex items-center gap-2">
                                      checkbox(attrs: attributes! {
                                          id=(id.clone())
                                          name="cor"
                                          value=(name.clone())
                                          checked=(selected)
                                      })
                                      if let Some(hex) = hex {
                                          <span class="size-4 rounded-full border border-border" style=(format!("background-color: {hex}"))></span>
                                      }
                                      <label for=(id) class="cursor-pointer text-sm">(name)</label>
                                  </div>
                              }
                          </div>
                      </fieldset>
                  }

                  <fieldset>
                      <legend class="mb-2 text-sm font-semibold">"Preço (R$)"</legend>
                      <div class="grid grid-cols-2 gap-2">
                          <div>
                              <label for="preco-min" class="text-xs text-muted-foreground">"De"</label>
                              input(attrs: attributes! {
                                  id="preco-min" name="preco_min" type="number" step="0.01" min="0"
                                  inputmode="decimal" placeholder="0"
                                  value=(preco_min.clone().unwrap_or_default())
                              })
                          </div>
                          <div>
                              <label for="preco-max" class="text-xs text-muted-foreground">"Até"</label>
                              input(attrs: attributes! {
                                  id="preco-max" name="preco_max" type="number" step="0.01" min="0"
                                  inputmode="decimal" placeholder="999"
                                  value=(preco_max.clone().unwrap_or_default())
                              })
                          </div>
                      </div>
                  </fieldset>

                  <fieldset class="space-y-2">
                      <legend class="mb-2 text-sm font-semibold">"Outros"</legend>
                      <div class="flex items-center gap-2">
                          checkbox(attrs: attributes! { id="disponivel" name="disponivel" value="true" checked=(disponivel) })
                          <label for="disponivel" class="cursor-pointer text-sm">"Somente disponíveis"</label>
                      </div>
                      <div class="flex items-center gap-2">
                          checkbox(attrs: attributes! { id="sale" name="sale" value="true" checked=(sale) })
                          <label for="sale" class="cursor-pointer text-sm">"Em promoção"</label>
                      </div>
                  </fieldset>
              </div>
              <div class="mt-4 flex gap-2">
                  <button type="submit" class=(button_variants(ButtonVariant::Primary, ButtonSize::Sm))>"Aplicar"</button>
                  <a href=(clear_url) class=(button_variants(ButtonVariant::Ghost, ButtonSize::Sm))>"Limpar"</a>
              </div>
          </details>
      </form>
  })
}

#[cfg(test)]
mod tests {
  use super::*;

  fn state() -> FilterState {
    FilterState {
      base: "/sets/verao".into(),
      category: "all".into(),
      search: "flor & mar".into(),
      sort: "novos".into(),
      sizes: vec!["p".into(), "m".into()],
      colors: vec!["Azul-marinho #2".into()],
      preco_min: Some("1&sale=true".into()),
      preco_max: None,
      disponivel: true,
      sale: false,
    }
  }

  #[test]
  fn href_encodes_values_and_keeps_base_path() {
    // `sale` is off in the fixture, so dropping it keeps every other param.
    let href = state().href(Drop::Sale);
    assert!(href.starts_with("/sets/verao?"));
    assert!(href.contains("search=flor+%26+mar"));
    assert!(href.contains("cor=Azul-marinho+%232"));
    // A crafted price can't smuggle in another param.
    assert!(href.contains("preco_min=1%26sale%3Dtrue"));
    assert!(!href.contains("&sale=true"));
  }

  #[test]
  fn repeated_values_are_collected_and_decoded() {
    let query = "tamanho=p&cor=Azul+marinho&tamanho=m&tamanho=&cor=Lim%C3%A3o";
    let pairs = query_pairs(query);
    assert_eq!(repeated_values(&pairs, "tamanho"), vec!["p", "m"]);
    assert_eq!(
      repeated_values(&pairs, "cor"),
      vec!["Azul marinho", "Limão"]
    );
    assert!(repeated_values(&query_pairs(""), "cor").is_empty());
  }

  #[test]
  fn href_drops_only_the_requested_filter() {
    let s = state();
    let href = s.href(Drop::Size("p"));
    assert!(href.contains("tamanho=m"));
    assert!(!href.contains("tamanho=p&"));
    let cleared = s.href(Drop::AllFacets);
    assert_eq!(cleared, "/sets/verao?search=flor+%26+mar&sort=novos");
  }
}
