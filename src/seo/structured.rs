//! Head builders for storefront pages: product / set Open Graph cards and
//! schema.org JSON-LD (`Product`, `Organization`, `WebSite`).

use serde_json::{Value, json};
use topcoat::context::Cx;

use super::{OpenGraph, PageHead, absolute_url, decimal_price, site_origin};
use crate::app::store::queries::{ProductDetail, SetSummary, category_label};

/// Brand name in the `Product` schema.
pub const BRAND: &str = "Lindaflor";
/// Instagram profile linked from the `Organization` schema.
const INSTAGRAM_URL: &str = "https://www.instagram.com/biquinislindaflor";

/// Percent-encodes one path segment (slugs are admin-entered).
pub fn encode_segment(segment: &str) -> String {
  let mut out = String::with_capacity(segment.len());
  for byte in segment.bytes() {
    if byte.is_ascii_alphanumeric() || b"-_.~".contains(&byte) {
      out.push(byte as char);
    } else {
      out.push_str(&format!("%{byte:02X}"));
    }
  }
  out
}

pub fn product_path(slug: &str) -> String {
  format!("/produtos/{}", encode_segment(slug))
}

pub fn set_path(slug: &str) -> String {
  format!("/sets/{}", encode_segment(slug))
}

/// Image URL for `og:image` / JSON-LD.
///
/// Product and set images live in S3 and are served through presigned GET
/// URLs (valid 24h, reused ~20h by the presign cache), already resolved by
/// the page. We publish that presigned URL as-is: it is absolute and loads
/// for crawlers today, and link-preview scrapers (WhatsApp, Facebook, X)
/// fetch and cache the image at share time. The trade-off: the URL expires,
/// so a crawler revisiting a cached card after 24h — or image search keyed
/// on the URL — sees a 403 until the page is re-crawled. A stable public
/// URL (public-read bucket/CDN, or an app route redirecting to a fresh
/// presign) would fix that and is the follow-up if social/image SEO matters.
/// Absolute `http(s)` URLs pass through; anything else is joined to the
/// site origin.
pub fn og_image_url(origin: &str, url: &str) -> Option<String> {
  let url = url.trim();
  if url.is_empty() {
    return None;
  }
  if url.starts_with("https://") || url.starts_with("http://") {
    Some(url.to_string())
  } else if url.starts_with('/') {
    Some(absolute_url(origin, url))
  } else {
    None
  }
}

fn availability_url(in_stock: bool) -> &'static str {
  if in_stock {
    "https://schema.org/InStock"
  } else {
    "https://schema.org/OutOfStock"
  }
}

/// Head for `/produtos/{slug}`: title, description, canonical, Open Graph
/// product card and a JSON-LD `Product` with a BRL `Offer`. Availability
/// comes from the variants' live availability (any variant with stock).
pub fn product_head(origin: &str, product: &ProductDetail) -> PageHead {
  let path = product_path(&product.slug);
  let url = absolute_url(origin, &path);
  let in_stock = product.variants.iter().any(|v| v.available > 0);
  let description = product
    .description
    .as_deref()
    .map(str::trim)
    .filter(|d| !d.is_empty())
    .map(str::to_owned)
    .unwrap_or_else(|| {
      format!(
        "{} — {} Linda Flor. Moda praia feita em Aracaju com envio para todo o Brasil.",
        product.name,
        category_label(&product.category)
      )
    });
  let images: Vec<String> = product
    .images
    .iter()
    .filter_map(|image| og_image_url(origin, &image.url))
    .collect();
  let sku = product
    .variants
    .iter()
    .find(|v| v.available > 0)
    .or_else(|| product.variants.first())
    .map(|v| v.sku.clone());

  let mut schema = json!({
    "@context": "https://schema.org",
    "@type": "Product",
    "name": product.name,
    "description": super::summarize(&description, 5000),
    "brand": { "@type": "Brand", "name": BRAND },
    "category": category_label(&product.category),
    "url": url,
    "offers": {
      "@type": "Offer",
      "url": url,
      "price": decimal_price(product.price_in_cents),
      "priceCurrency": "BRL",
      "availability": availability_url(in_stock),
      "itemCondition": "https://schema.org/NewCondition",
    },
  });
  if !images.is_empty() {
    schema["image"] = Value::from(images.clone());
  }
  if let Some(sku) = sku {
    schema["sku"] = Value::from(sku);
  }

  PageHead::new(&product.name)
    .description(&description)
    .canonical(path)
    .open_graph(OpenGraph {
      kind: "product",
      image: images.first().cloned(),
      image_alt: product
        .images
        .first()
        .and_then(|image| image.alt.clone())
        .or_else(|| Some(product.name.clone())),
      price_cents: Some(product.price_in_cents),
      in_stock: Some(in_stock),
    })
    .json_ld(schema)
}

/// Head for `/sets/{slug}`: title, description, canonical and an Open Graph
/// product card (bundle price when the set has one).
pub fn set_head(
  origin: &str,
  set: &SetSummary,
  cover_url: Option<&str>,
  in_stock: bool,
) -> PageHead {
  let description = set
    .description
    .as_deref()
    .map(str::trim)
    .filter(|d| !d.is_empty())
    .map(str::to_owned)
    .unwrap_or_else(|| {
      format!(
        "Conjunto {} Linda Flor: top e calcinha combinando, com preço especial.",
        set.name
      )
    });
  PageHead::new(&set.name)
    .description(description)
    .canonical(set_path(&set.slug))
    .open_graph(OpenGraph {
      kind: "product",
      image: cover_url.and_then(|url| og_image_url(origin, url)),
      image_alt: Some(set.name.clone()),
      price_cents: set.price_in_cents,
      in_stock: Some(in_stock),
    })
}

/// Head for the home page: default title/description, Open Graph website
/// card and JSON-LD `Organization` + `WebSite` (with a `SearchAction` into
/// the catalog search, whose query parameter is `search`).
pub fn home_head(origin: &str) -> PageHead {
  let home = absolute_url(origin, "/");
  PageHead::site_default()
    .canonical("/")
    .open_graph(OpenGraph {
      kind: "website",
      ..OpenGraph::default()
    })
    .json_ld(json!({
      "@context": "https://schema.org",
      "@type": "Organization",
      "name": super::SITE_NAME,
      "url": home,
      "sameAs": [INSTAGRAM_URL],
    }))
    .json_ld(json!({
      "@context": "https://schema.org",
      "@type": "WebSite",
      "name": super::SITE_NAME,
      "url": home,
      "inLanguage": "pt-BR",
      "potentialAction": {
        "@type": "SearchAction",
        "target": {
          "@type": "EntryPoint",
          "urlTemplate": format!("{}?search={{search_term_string}}", absolute_url(origin, "/produtos")),
        },
        "query-input": "required name=search_term_string",
      },
    }))
}

/// Sets the product page head (see [`product_head`]).
pub fn set_product_head(cx: &Cx, product: &ProductDetail) {
  super::set_head(cx, product_head(&site_origin(cx), product));
}

/// Sets the set page head (see [`set_head`]).
pub fn set_set_head(
  cx: &Cx,
  set: &SetSummary,
  cover_url: Option<&str>,
  in_stock: bool,
) {
  super::set_head(cx, set_head(&site_origin(cx), set, cover_url, in_stock));
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::app::store::queries::{ProductImage, ProductVariant};
  use topcoat::view::ViewExt;
  use uuid::Uuid;

  const ORIGIN: &str = "https://loja.example.com";

  fn product(available: i32) -> ProductDetail {
    ProductDetail {
      id: Uuid::nil(),
      name: "Biquíni </script><script>alert(1)</script>".to_string(),
      slug: "biquini-sol".to_string(),
      description: Some("Top \"cortininha\" & calcinha <lacinho>.".to_string()),
      price_in_cents: 8990,
      compare_at_price_in_cents: None,
      category: "biquini".to_string(),
      images: vec![ProductImage {
        id: Uuid::nil(),
        url: "https://s3.example.com/lf/p/1.jpg?X-Amz-Signature=abc&X-Amz-Expires=86400".to_string(),
        alt: Some("Biquíni Sol".to_string()),
      }],
      variants: vec![ProductVariant {
        id: Uuid::nil(),
        sku: "BQ-SOL-M".to_string(),
        size: "m".to_string(),
        color: "Azul".to_string(),
        price_in_cents: None,
        available,
      }],
      colors: vec![],
    }
  }

  async fn render_head(head: PageHead, path: &str) -> String {
    let cx = Cx::default();
    super::super::head_tags(
      &cx,
      super::super::resolve_head(Some(head), false, path, ORIGIN),
    )
    .first()
    .await
    .expect("render")
    .render(&cx)
  }

  fn json_ld_blocks(html: &str) -> Vec<serde_json::Value> {
    let open = r#"<script type="application/ld+json">"#;
    html
      .split(open)
      .skip(1)
      .map(|rest| {
        let body = rest.split("</script>").next().unwrap();
        serde_json::from_str(body).expect("valid JSON-LD")
      })
      .collect()
  }

  #[test]
  fn product_schema_has_offer_in_brl() {
    let head = product_head(ORIGIN, &product(3));
    let schema = &head.json_ld[0];
    assert_eq!(schema["@type"], "Product");
    assert_eq!(schema["brand"]["name"], "Lindaflor");
    assert_eq!(schema["sku"], "BQ-SOL-M");
    assert_eq!(
      schema["url"],
      "https://loja.example.com/produtos/biquini-sol"
    );
    assert_eq!(schema["offers"]["price"], "89.90");
    assert_eq!(schema["offers"]["priceCurrency"], "BRL");
    assert_eq!(
      schema["offers"]["availability"],
      "https://schema.org/InStock"
    );
    assert!(schema["image"][0].as_str().unwrap().starts_with("https://"));

    let sold_out = product_head(ORIGIN, &product(0));
    assert_eq!(
      sold_out.json_ld[0]["offers"]["availability"],
      "https://schema.org/OutOfStock"
    );
  }

  #[tokio::test]
  async fn product_page_head_contains_expected_tags() {
    let html =
      render_head(product_head(ORIGIN, &product(2)), "/produtos/biquini-sol")
        .await;
    for needle in [
      "<title>Biquíni &lt;/script&gt;&lt;script&gt;alert(1)&lt;/script&gt; | Linda Flor</title>",
      r#"<meta name="description" content="Top &quot;cortininha&quot; &amp; calcinha <lacinho>.">"#,
      r#"<link rel="canonical" href="https://loja.example.com/produtos/biquini-sol">"#,
      r#"<meta property="og:type" content="product">"#,
      r#"<meta property="og:url" content="https://loja.example.com/produtos/biquini-sol">"#,
      r#"<meta property="og:image" content="https://s3.example.com/lf/p/1.jpg?X-Amz-Signature=abc&amp;X-Amz-Expires=86400">"#,
      r#"<meta property="product:price:amount" content="89.90">"#,
      r#"<meta property="product:price:currency" content="BRL">"#,
      r#"<meta property="product:availability" content="in stock">"#,
      r#"<meta name="twitter:card" content="summary_large_image">"#,
      r#"<script type="application/ld+json">"#,
    ] {
      assert!(html.contains(needle), "missing {needle}\n{html}");
    }
    assert!(!html.contains("robots"), "{html}");

    // The first `</script>` after the JSON-LD opening tag is the element's
    // own closing tag at the very end: the product name's `</script>` stays
    // inside the JSON as `</script`. (In quoted attribute values such
    // as og:title it is inert and only `"`/`&` need escaping.)
    let (_, ld) = html
      .split_once(r#"<script type="application/ld+json">"#)
      .unwrap();
    assert_eq!(ld.find("</script>"), Some(ld.len() - "</script>".len()));
    assert!(!ld.trim_end_matches("</script>").contains('<'), "{ld}");
    let blocks = json_ld_blocks(&html);
    assert_eq!(blocks.len(), 1);
    assert_eq!(
      blocks[0]["name"],
      "Biquíni </script><script>alert(1)</script>"
    );
  }

  #[tokio::test]
  async fn home_head_has_organization_and_website_search() {
    let html = render_head(home_head(ORIGIN), "/").await;
    assert!(
      html.contains("<title>Linda Flor — Moda Praia</title>"),
      "{html}"
    );
    assert!(
      html
        .contains(r#"<link rel="canonical" href="https://loja.example.com/">"#),
      "{html}"
    );
    let blocks = json_ld_blocks(&html);
    assert_eq!(blocks.len(), 2);
    assert_eq!(blocks[0]["@type"], "Organization");
    assert_eq!(blocks[1]["@type"], "WebSite");
    assert_eq!(
      blocks[1]["potentialAction"]["target"]["urlTemplate"],
      "https://loja.example.com/produtos?search={search_term_string}"
    );
  }

  #[test]
  fn slugs_are_percent_encoded() {
    assert_eq!(product_path("biquini-sol"), "/produtos/biquini-sol");
    assert_eq!(set_path("verão 26"), "/sets/ver%C3%A3o%2026");
  }

  #[test]
  fn og_image_url_keeps_absolute_and_joins_relative() {
    assert_eq!(
      og_image_url(ORIGIN, "/img/a.jpg").as_deref(),
      Some("https://loja.example.com/img/a.jpg")
    );
    assert_eq!(og_image_url(ORIGIN, "products/a.jpg"), None);
    assert_eq!(og_image_url(ORIGIN, ""), None);
  }
}
