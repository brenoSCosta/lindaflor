//! `GET /robots.txt` and `GET /sitemap.xml`.
//!
//! `robots.txt` is static (exempt from the global rate limit like bundled
//! assets). The sitemap lists the home page, the catalog and its category
//! pages, sets, the static pages and every active product and set with its
//! `lastmod`; it is built from Postgres and cached in-process for
//! [`SITEMAP_CACHE_TTL`] so crawler bursts cost one query set.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use http::{
  HeaderValue,
  header::{CACHE_CONTROL, CONTENT_TYPE},
};
use sqlx::PgPool;
use topcoat::{
  Result,
  context::{Cx, app_context},
  router::{response::IntoResponse, route},
};

use super::site_origin;
use super::structured::{product_path, set_path};

pub const ROBOTS_PATH: &str = "/robots.txt";
pub const SITEMAP_PATH: &str = "/sitemap.xml";

/// How long a built sitemap is served before it is rebuilt.
pub const SITEMAP_CACHE_TTL: Duration = Duration::from_secs(5 * 60);

/// Areas crawlers must not fetch (private pages also carry `noindex`).
const DISALLOWED: &[&str] = &[
  "/admin",
  "/conta",
  "/checkout",
  "/carrinho",
  "/pedido",
  "/api",
  "/settings",
];

/// Indexable static pages: (path, changefreq, priority).
const STATIC_PAGES: &[(&str, &str, &str)] = &[
  ("/", "daily", "1.0"),
  ("/produtos", "daily", "0.9"),
  ("/sets", "weekly", "0.7"),
  ("/guia-de-medidas", "yearly", "0.4"),
  ("/trocas-devolucoes", "yearly", "0.3"),
  ("/termos", "yearly", "0.2"),
  ("/politica-privacidade", "yearly", "0.2"),
];

pub fn robots_body(origin: &str) -> String {
  let mut out = String::from("User-agent: *\n");
  for path in DISALLOWED {
    out.push_str(&format!("Disallow: {path}\n"));
  }
  out.push_str(&format!("\nSitemap: {origin}{SITEMAP_PATH}\n"));
  out
}

#[route(GET "/robots.txt")]
pub async fn robots_txt(cx: &Cx) -> Result<impl IntoResponse> {
  Ok((
    [
      (
        CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=utf-8"),
      ),
      (
        CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=3600"),
      ),
    ],
    robots_body(&site_origin(cx)),
  ))
}

/// One `<url>` entry.
#[derive(Debug, Clone, PartialEq)]
pub struct SitemapEntry {
  /// Root-relative path (+ query).
  pub path: String,
  /// `YYYY-MM-DD`.
  pub lastmod: Option<String>,
  pub changefreq: &'static str,
  pub priority: &'static str,
}

fn xml_escape(value: &str) -> String {
  let mut out = String::with_capacity(value.len());
  for c in value.chars() {
    match c {
      '&' => out.push_str("&amp;"),
      '<' => out.push_str("&lt;"),
      '>' => out.push_str("&gt;"),
      '"' => out.push_str("&quot;"),
      '\'' => out.push_str("&apos;"),
      c => out.push(c),
    }
  }
  out
}

pub fn render_sitemap(origin: &str, entries: &[SitemapEntry]) -> String {
  let mut out = String::from(
    "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
     <urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n",
  );
  for entry in entries {
    out.push_str("  <url><loc>");
    out.push_str(&xml_escape(&super::absolute_url(origin, &entry.path)));
    out.push_str("</loc>");
    if let Some(lastmod) = &entry.lastmod {
      out.push_str("<lastmod>");
      out.push_str(&xml_escape(lastmod));
      out.push_str("</lastmod>");
    }
    out.push_str("<changefreq>");
    out.push_str(entry.changefreq);
    out.push_str("</changefreq><priority>");
    out.push_str(entry.priority);
    out.push_str("</priority></url>\n");
  }
  out.push_str("</urlset>\n");
  out
}

/// Every indexable URL: static pages, category pages with active products,
/// then active products and active sets (with `lastmod`).
pub async fn sitemap_entries(
  pool: &PgPool,
) -> Result<Vec<SitemapEntry>, sqlx::Error> {
  let mut entries: Vec<SitemapEntry> = STATIC_PAGES
    .iter()
    .map(|(path, changefreq, priority)| SitemapEntry {
      path: (*path).to_string(),
      lastmod: None,
      changefreq,
      priority,
    })
    .collect();

  let categories: Vec<String> = sqlx::query_scalar(
    "SELECT DISTINCT category::text FROM products WHERE active = true ORDER BY 1",
  )
  .fetch_all(pool)
  .await?;
  entries.extend(categories.into_iter().map(|category| SitemapEntry {
    path: format!("/produtos?category={category}"),
    lastmod: None,
    changefreq: "daily",
    priority: "0.8",
  }));

  let products: Vec<(String, String)> = sqlx::query_as(
    "SELECT slug, to_char(updated_at, 'YYYY-MM-DD')
       FROM products WHERE active = true
      ORDER BY updated_at DESC, slug ASC",
  )
  .fetch_all(pool)
  .await?;
  entries.extend(products.into_iter().map(|(slug, lastmod)| SitemapEntry {
    path: product_path(&slug),
    lastmod: Some(lastmod),
    changefreq: "weekly",
    priority: "0.8",
  }));

  let sets: Vec<(String, String)> = sqlx::query_as(
    "SELECT slug, to_char(updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD')
       FROM sets WHERE active = true
      ORDER BY updated_at DESC, slug ASC",
  )
  .fetch_all(pool)
  .await?;
  entries.extend(sets.into_iter().map(|(slug, lastmod)| SitemapEntry {
    path: set_path(&slug),
    lastmod: Some(lastmod),
    changefreq: "weekly",
    priority: "0.6",
  }));

  Ok(entries)
}

struct CachedSitemap {
  origin: String,
  built_at: Instant,
  xml: String,
}

static SITEMAP_CACHE: Mutex<Option<CachedSitemap>> = Mutex::new(None);

fn cached_sitemap(origin: &str) -> Option<String> {
  let cache = SITEMAP_CACHE.lock().unwrap_or_else(|e| e.into_inner());
  cache
    .as_ref()
    .filter(|c| c.origin == origin && c.built_at.elapsed() < SITEMAP_CACHE_TTL)
    .map(|c| c.xml.clone())
}

#[route(GET "/sitemap.xml")]
pub async fn sitemap_xml(cx: &Cx) -> Result<impl IntoResponse> {
  let origin = site_origin(cx);
  let xml = match cached_sitemap(&origin) {
    Some(xml) => xml,
    None => {
      let entries = sitemap_entries(app_context::<PgPool>(cx)).await?;
      let xml = render_sitemap(&origin, &entries);
      *SITEMAP_CACHE.lock().unwrap_or_else(|e| e.into_inner()) =
        Some(CachedSitemap {
          origin,
          built_at: Instant::now(),
          xml: xml.clone(),
        });
      xml
    }
  };
  Ok((
    [
      (
        CONTENT_TYPE,
        HeaderValue::from_static("application/xml; charset=utf-8"),
      ),
      (
        CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=300"),
      ),
    ],
    xml,
  ))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn robots_disallows_private_areas_and_points_to_sitemap() {
    let body = robots_body("https://loja.example.com");
    for path in DISALLOWED {
      assert!(body.contains(&format!("Disallow: {path}\n")), "{body}");
    }
    assert!(body.starts_with("User-agent: *\n"), "{body}");
    assert!(
      body.contains("Sitemap: https://loja.example.com/sitemap.xml"),
      "{body}"
    );
  }

  #[test]
  fn sitemap_escapes_locations() {
    let xml = render_sitemap(
      "https://loja.example.com",
      &[SitemapEntry {
        path: "/produtos?category=maio&x=<y>".to_string(),
        lastmod: Some("2026-10-22".to_string()),
        changefreq: "daily",
        priority: "0.8",
      }],
    );
    assert!(
      xml.contains(
        "<loc>https://loja.example.com/produtos?category=maio&amp;x=&lt;y&gt;</loc>"
      ),
      "{xml}"
    );
    assert!(xml.contains("<lastmod>2026-10-22</lastmod>"), "{xml}");
  }
}
