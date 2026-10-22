//! Per-page `<head>` metadata (title, description, canonical, robots, Open
//! Graph / Twitter cards, JSON-LD) plus `robots.txt` / `sitemap.xml`.
//!
//! Topcoat 0.9 has no head/title facility: the root layout writes `<head>`
//! before the page body, and a layout cannot see what the page loaded. The
//! bridge relies on how Topcoat renders a template — dynamic nodes resolve
//! concurrently and the document is assembled once every node has content
//! (`JoinView`; `Child` docs: children "render concurrently with the
//! surrounding content"). So:
//!
//! - pages call [`set_head`] (usually right after loading their data);
//! - the root layout renders [`page_head`] in `<head>`, which waits until the
//!   page's first content resolved (tracked by wrapping the slot with
//!   [`track_slot`]), then renders the last head the page set. Waiting costs
//!   nothing: the document only goes out once every node has content.
//!
//! The shared state is a per-request `#[memoize]` value, so the page and the
//! layout reach the same cell without threading anything through the slot.
//! Pages that never call [`set_head`] get the site default title; private
//! areas get `noindex` from [`is_private_path`] regardless of the page.

pub mod sitemap;
pub mod structured;

use std::future::poll_fn;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};

use topcoat::{
  Result,
  context::{Cx, memoize},
  router::{Slot, request},
  view::{
    Child, Unescaped, View, ViewExt, ViewFirst, ViewSwap, component, view,
  },
};

/// Brand shown in titles and `og:site_name`.
pub const SITE_NAME: &str = "Linda Flor";
/// Title of pages that set no head (and of the home page).
pub const DEFAULT_TITLE: &str = "Linda Flor — Moda Praia";
/// Fallback meta description (home page and pages without their own).
pub const DEFAULT_DESCRIPTION: &str = "Moda praia feita com carinho em Aracaju: biquínis, maiôs, saídas de praia e acessórios Linda Flor. Frete para todo o Brasil e desconto no Pix.";
/// Meta descriptions are cut to this many characters (search snippets show
/// roughly 150–160).
const DESCRIPTION_MAX_CHARS: usize = 160;

/// Path prefixes that must never be indexed: account, checkout, orders,
/// admin and the auth flows. Matched on whole segments (`/conta` covers
/// `/conta/pedidos`, not `/contato`).
const PRIVATE_PREFIXES: &[&str] = &[
  "/admin",
  "/carrinho",
  "/check-email",
  "/checkout",
  "/confirmar-email",
  "/conta",
  "/dashboard",
  "/excluir-conta",
  "/favoritos",
  "/forgot-password",
  "/login",
  "/logout",
  "/newsletter",
  "/pedido",
  "/reset-password",
  "/settings",
  "/signup",
  "/two-factor",
  "/verify-email",
];

/// Whether `path` belongs to a private (never indexed) area.
pub fn is_private_path(path: &str) -> bool {
  PRIVATE_PREFIXES.iter().any(|prefix| {
    path
      .strip_prefix(prefix)
      .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
  })
}

/// Open Graph / Twitter card data for a shareable page.
#[derive(Clone, Debug, Default)]
pub struct OpenGraph {
  /// `og:type`: `website` or `product`.
  pub kind: &'static str,
  /// Absolute image URL (see [`structured::og_image_url`] for the
  /// presigned-URL trade-off).
  pub image: Option<String>,
  pub image_alt: Option<String>,
  /// Price in cents (BRL) for `product:price:amount`.
  pub price_cents: Option<i64>,
  /// `product:availability`.
  pub in_stock: Option<bool>,
}

/// Head metadata a page hands to the root layout via [`set_head`].
#[derive(Clone, Debug, Default)]
pub struct PageHead {
  /// Full `<title>` text.
  pub title: String,
  pub description: Option<String>,
  /// Canonical path (+ query) relative to the site root, e.g. `/produtos`.
  /// Rendered as an absolute URL on the configured origin.
  pub canonical: Option<String>,
  pub noindex: bool,
  pub open_graph: Option<OpenGraph>,
  /// JSON-LD documents, each rendered in its own
  /// `<script type="application/ld+json">`.
  pub json_ld: Vec<serde_json::Value>,
}

impl PageHead {
  /// A head titled `"{title} | Linda Flor"`.
  pub fn new(title: impl AsRef<str>) -> Self {
    Self {
      title: format!("{} | {SITE_NAME}", title.as_ref()),
      ..Self::default()
    }
  }

  /// The site-wide default head (home page).
  pub fn site_default() -> Self {
    Self {
      title: DEFAULT_TITLE.to_string(),
      description: Some(DEFAULT_DESCRIPTION.to_string()),
      ..Self::default()
    }
  }

  /// Sets the meta description (whitespace collapsed, cut at
  /// [`DESCRIPTION_MAX_CHARS`]). Blank text leaves it unset.
  #[must_use]
  pub fn description(mut self, text: impl AsRef<str>) -> Self {
    let text = summarize(text.as_ref(), DESCRIPTION_MAX_CHARS);
    self.description = (!text.is_empty()).then_some(text);
    self
  }

  #[must_use]
  pub fn canonical(mut self, path: impl Into<String>) -> Self {
    self.canonical = Some(path.into());
    self
  }

  #[must_use]
  pub fn noindex(mut self) -> Self {
    self.noindex = true;
    self
  }

  #[must_use]
  pub fn open_graph(mut self, og: OpenGraph) -> Self {
    self.open_graph = Some(og);
    self
  }

  #[must_use]
  pub fn json_ld(mut self, document: serde_json::Value) -> Self {
    self.json_ld.push(document);
    self
  }
}

/// Collapses whitespace and truncates to `max` chars (with an ellipsis).
pub fn summarize(text: &str, max: usize) -> String {
  let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
  if collapsed.chars().count() <= max {
    return collapsed;
  }
  let cut: String = collapsed.chars().take(max.saturating_sub(1)).collect();
  format!("{}…", cut.trim_end())
}

#[derive(Default)]
struct HeadSlotInner {
  head: Option<PageHead>,
  /// The page's first content resolved (or failed).
  done: bool,
  /// The page failed (404 and friends): never index the error page.
  failed: bool,
  waker: Option<Waker>,
}

/// Per-request cell shared by the page ([`set_head`]), the slot tracker and
/// the layout's [`page_head`].
#[derive(Clone, Default)]
pub struct HeadSlot(Arc<Mutex<HeadSlotInner>>);

impl HeadSlot {
  fn update(&self, f: impl FnOnce(&mut HeadSlotInner)) {
    let waker = {
      let mut inner = self.0.lock().unwrap_or_else(|e| e.into_inner());
      f(&mut inner);
      inner.waker.take()
    };
    if let Some(waker) = waker {
      waker.wake();
    }
  }

  /// Resolves once the page finished, yielding the last head it set (if
  /// any) and whether the page failed. Not on the first `set_head`: a page
  /// may set a provisional head (e.g. not-found) and replace it later.
  async fn wait(&self) -> (Option<PageHead>, bool) {
    poll_fn(|task| {
      let mut inner = self.0.lock().unwrap_or_else(|e| e.into_inner());
      if inner.done {
        Poll::Ready((inner.head.clone(), inner.failed))
      } else {
        inner.waker = Some(task.waker().clone());
        Poll::Pending
      }
    })
    .await
  }
}

#[memoize]
fn head_slot(cx: &Cx) -> HeadSlot {
  let _ = cx;
  HeadSlot::default()
}

/// Sets this request's `<head>` metadata. Call from a page handler; the last
/// call before the page's first content resolves wins.
pub fn set_head(cx: &Cx, head: PageHead) {
  head_slot(cx)
    .clone()
    .update(|inner| inner.head = Some(head));
}

/// Wraps the root layout's slot so the head cell is marked done once the
/// page's first content resolves: [`page_head`] never waits on a page that
/// did not call [`set_head`].
pub fn track_slot<'a>(cx: &Cx, slot: Slot<'a>) -> Slot<'a> {
  Child::new(TrackedSlot {
    slot,
    state: head_slot(cx).clone(),
  })
}

struct TrackedSlot<'a> {
  slot: Slot<'a>,
  state: HeadSlot,
}

impl View for TrackedSlot<'_> {
  fn poll_first(
    self: Pin<&mut Self>,
    cx: &mut Context<'_>,
  ) -> Poll<Result<ViewFirst>> {
    let this = self.get_mut();
    let poll = Pin::new(&mut this.slot).poll_first(cx);
    if let Poll::Ready(result) = &poll {
      let failed = result.is_err();
      this.state.update(|inner| {
        inner.done = true;
        inner.failed = failed;
      });
    }
    poll
  }

  fn poll_swap(
    self: Pin<&mut Self>,
    cx: &mut Context<'_>,
  ) -> Poll<Result<Option<ViewSwap>>> {
    Pin::new(&mut self.get_mut().slot).poll_swap(cx)
  }
}

/// Public origin for absolute URLs: `APP_ORIGIN` (required in production),
/// else the request `Host` (dev), else `http://localhost`.
pub fn site_origin(cx: &Cx) -> String {
  if let Some(origin) = crate::auth::google::app_origin() {
    return origin;
  }
  request::headers(cx)
    .get(http::header::HOST)
    .and_then(|value| value.to_str().ok())
    .filter(|host| {
      !host.is_empty()
        && host
          .chars()
          .all(|c| c.is_ascii_alphanumeric() || ".-:[]".contains(c))
    })
    .map(|host| format!("http://{host}"))
    .unwrap_or_else(|| "http://localhost".to_string())
}

/// Joins the origin and a root-relative path.
pub fn absolute_url(origin: &str, path: &str) -> String {
  if path.contains("://") {
    return path.to_string();
  }
  if path.starts_with('/') {
    format!("{origin}{path}")
  } else {
    format!("{origin}/{path}")
  }
}

/// Serializes a JSON-LD document for embedding in
/// `<script type="application/ld+json">`. `<`, `>` and `&` (plus U+2028/9)
/// are emitted as `\uXXXX` escapes — still the same JSON — so text such as
/// `</script>` or `<!--` in product data cannot close or alter the element.
pub fn json_ld_script_body(document: &serde_json::Value) -> String {
  let json = serde_json::to_string(document).unwrap_or_else(|_| "{}".into());
  let mut out = String::with_capacity(json.len());
  for c in json.chars() {
    match c {
      '<' => out.push_str("\\u003c"),
      '>' => out.push_str("\\u003e"),
      '&' => out.push_str("\\u0026"),
      '\u{2028}' => out.push_str("\\u2028"),
      '\u{2029}' => out.push_str("\\u2029"),
      c => out.push(c),
    }
  }
  out
}

/// Formats cents as a schema.org / Open Graph decimal (`8990` → `89.90`).
pub fn decimal_price(cents: i64) -> String {
  let sign = if cents < 0 { "-" } else { "" };
  let cents = cents.unsigned_abs();
  format!("{sign}{}.{:02}", cents / 100, cents % 100)
}

/// Everything [`head_tags`] renders, resolved to final strings.
struct ResolvedHead {
  title: String,
  description: Option<String>,
  canonical: Option<String>,
  robots: Option<&'static str>,
  og: Option<ResolvedOpenGraph>,
  json_ld: Vec<String>,
}

struct ResolvedOpenGraph {
  kind: &'static str,
  title: String,
  description: Option<String>,
  url: Option<String>,
  image: Option<String>,
  image_alt: Option<String>,
  price: Option<String>,
  availability: Option<&'static str>,
}

fn resolve_head(
  head: Option<PageHead>,
  failed: bool,
  path: &str,
  origin: &str,
) -> ResolvedHead {
  let private = is_private_path(path);
  let head = match head {
    // A failed page (404 boundary) renders the error page: never index it
    // or advertise the page's canonical / cards.
    Some(_) | None if failed => PageHead {
      noindex: true,
      ..PageHead::new("Página não encontrada")
    },
    Some(head) => head,
    None => PageHead {
      title: DEFAULT_TITLE.to_string(),
      ..PageHead::default()
    },
  };
  let noindex = head.noindex || private;
  let canonical = (!noindex)
    .then(|| head.canonical.as_deref().map(|p| absolute_url(origin, p)))
    .flatten();
  let og = head
    .open_graph
    .filter(|_| !noindex)
    .map(|og| ResolvedOpenGraph {
      kind: if og.kind.is_empty() {
        "website"
      } else {
        og.kind
      },
      title: head.title.clone(),
      description: head.description.clone(),
      url: canonical.clone(),
      image: og.image.map(|image| absolute_url(origin, &image)),
      image_alt: og.image_alt,
      price: og.price_cents.map(decimal_price),
      availability: og
        .in_stock
        .map(|in_stock| if in_stock { "in stock" } else { "out of stock" }),
    });
  ResolvedHead {
    title: head.title,
    description: head.description,
    canonical,
    robots: noindex.then_some("noindex, nofollow"),
    og,
    json_ld: if noindex {
      Vec::new()
    } else {
      head.json_ld.iter().map(json_ld_script_body).collect()
    },
  }
}

/// Renders resolved head tags (title, description, canonical, robots, Open
/// Graph, Twitter card, JSON-LD).
fn head_tags(cx: &Cx, head: ResolvedHead) -> impl View {
  let ResolvedHead {
    title,
    description,
    canonical,
    robots,
    og,
    json_ld,
  } = head;
  view! { cx =>
      <title>(title)</title>
      if let Some(description) = description {
          <meta name="description" content=(description)>
      }
      if let Some(canonical) = canonical {
          <link rel="canonical" href=(canonical)>
      }
      if let Some(robots) = robots {
          <meta name="robots" content=(robots)>
      }
      if let Some(og) = og {
          <meta property="og:site_name" content=(SITE_NAME)>
          <meta property="og:locale" content="pt_BR">
          <meta property="og:type" content=(og.kind)>
          <meta property="og:title" content=(og.title.clone())>
          if let Some(description) = og.description.clone() {
              <meta property="og:description" content=(description)>
          }
          if let Some(url) = og.url {
              <meta property="og:url" content=(url)>
          }
          if let Some(image) = og.image.clone() {
              <meta property="og:image" content=(image)>
              if let Some(alt) = og.image_alt {
                  <meta property="og:image:alt" content=(alt)>
              }
          }
          if let Some(price) = og.price {
              <meta property="product:price:amount" content=(price.clone())>
              <meta property="product:price:currency" content="BRL">
              <meta property="og:price:amount" content=(price)>
              <meta property="og:price:currency" content="BRL">
          }
          if let Some(availability) = og.availability {
              <meta property="product:availability" content=(availability)>
          }
          <meta
              name="twitter:card"
              content=(if og.image.is_some() { "summary_large_image" } else { "summary" })
          >
          <meta name="twitter:title" content=(og.title)>
          if let Some(description) = og.description {
              <meta name="twitter:description" content=(description)>
          }
          if let Some(image) = og.image {
              <meta name="twitter:image" content=(image)>
          }
      }
      for body in json_ld {
          <script type="application/ld+json">(Unescaped::new_unchecked(body))</script>
      }
  }
}

/// The root layout's per-page head tags. Waits for the page's
/// [`set_head`] (or for the page to finish), then renders.
#[component]
pub async fn page_head(cx: &Cx) -> Result<impl View> {
  let (head, failed) = head_slot(cx).clone().wait().await;
  let path = request::uri(cx).path().to_owned();
  let origin = site_origin(cx);
  Ok(head_tags(cx, resolve_head(head, failed, &path, &origin)).boxed())
}

#[cfg(test)]
mod tests {
  use super::*;
  use topcoat::view::ViewExt;

  async fn render(head: Option<PageHead>, failed: bool, path: &str) -> String {
    let cx = Cx::default();
    head_tags(
      &cx,
      resolve_head(head, failed, path, "https://loja.example.com"),
    )
    .first()
    .await
    .expect("render head")
    .render(&cx)
  }

  #[test]
  fn private_paths_match_whole_segments() {
    for path in [
      "/admin",
      "/admin/produtos",
      "/conta/pedidos",
      "/checkout",
      "/pedido/0199c1a0-0000-7000-8000-000000000001",
      "/login",
      "/settings/two-factor",
    ] {
      assert!(is_private_path(path), "{path}");
    }
    for path in ["/", "/produtos", "/contato", "/sets/verao", "/termos"] {
      assert!(!is_private_path(path), "{path}");
    }
  }

  #[test]
  fn json_ld_body_cannot_close_the_script_element() {
    let doc = serde_json::json!({
      "name": "Biquíni </script><script>alert(1)</script> & <!-- \u{2028}",
    });
    let body = json_ld_script_body(&doc);
    assert!(!body.contains('<'), "{body}");
    assert!(!body.contains('>'), "{body}");
    assert!(!body.contains('&'), "{body}");
    assert!(!body.to_lowercase().contains("</script"), "{body}");
    assert!(body.contains("\\u003c/script\\u003e"), "{body}");
    // Still the same JSON once parsed.
    let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(parsed, doc);
  }

  #[test]
  fn decimal_price_formats_cents() {
    assert_eq!(decimal_price(8990), "89.90");
    assert_eq!(decimal_price(5), "0.05");
    assert_eq!(decimal_price(100_000), "1000.00");
  }

  #[test]
  fn summarize_collapses_and_truncates() {
    assert_eq!(summarize("  a \n b  ", 10), "a b");
    let long = "á".repeat(200);
    let out = summarize(&long, 160);
    assert_eq!(out.chars().count(), 160);
    assert!(out.ends_with('…'));
  }

  #[tokio::test]
  async fn default_head_has_site_title_and_no_robots() {
    let html = render(None, false, "/sets").await;
    assert!(
      html.contains("<title>Linda Flor — Moda Praia</title>"),
      "{html}"
    );
    assert!(!html.contains("robots"), "{html}");
  }

  #[tokio::test]
  async fn private_paths_are_noindex_even_with_a_head() {
    let head = PageHead::new("Checkout").canonical("/checkout");
    let html = render(Some(head), false, "/checkout").await;
    assert!(
      html.contains(r#"<meta name="robots" content="noindex, nofollow">"#),
      "{html}"
    );
    assert!(!html.contains("canonical"), "{html}");
  }

  #[tokio::test]
  async fn failed_page_is_noindex() {
    let head = PageHead::new("Produto").canonical("/produtos/x");
    let html = render(Some(head), true, "/produtos/x").await;
    assert!(html.contains("noindex"), "{html}");
    assert!(html.contains("Página não encontrada"), "{html}");
    assert!(!html.contains("canonical"), "{html}");
  }

  #[tokio::test]
  async fn attribute_values_are_escaped() {
    let head = PageHead::new(r#"Biquíni "Sol" <b>"#)
      .description(r#"Top "lindo" & <confortável>"#)
      .canonical("/produtos/sol");
    let html = render(Some(head), false, "/produtos/sol").await;
    assert!(html.contains("&quot;lindo&quot; &amp;"), "{html}");
    assert!(!html.contains("<b>"), "{html}");
    assert!(
      html.contains(
        r#"<link rel="canonical" href="https://loja.example.com/produtos/sol">"#
      ),
      "{html}"
    );
  }

  #[tokio::test]
  async fn head_wait_resolves_when_page_is_done_with_last_head() {
    let cx = Cx::default();
    let state = head_slot(&cx).clone();
    let waiter = tokio::spawn({
      let state = state.clone();
      async move { state.wait().await }
    });
    tokio::task::yield_now().await;
    set_head(&cx, PageHead::new("Produto não encontrado").noindex());
    tokio::task::yield_now().await;
    assert!(
      !waiter.is_finished(),
      "must not resolve on the first set_head"
    );
    set_head(&cx, PageHead::new("Termos"));
    state.update(|inner| inner.done = true);
    let (head, failed) = waiter.await.unwrap();
    let head = head.unwrap();
    assert_eq!(head.title, "Termos | Linda Flor");
    assert!(!head.noindex);
    assert!(!failed);

    let cx = Cx::default();
    let state = head_slot(&cx).clone();
    state.update(|inner| {
      inner.done = true;
      inner.failed = true;
    });
    let (head, failed) = state.wait().await;
    assert!(head.is_none());
    assert!(failed);
  }
}
