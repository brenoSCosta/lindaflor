use http::{Method, Request, StatusCode, header};
use test_support::{body_bytes, pool_and_router};
use topcoat::router::{Body, Router};
use uuid::Uuid;

const HOST: &str = "loja.test";

fn expected_origin() -> String {
  lindaflor::auth::google::app_origin()
    .unwrap_or_else(|| format!("http://{HOST}"))
}

async fn get(router: &Router, path: &str) -> (StatusCode, String, String) {
  // Link the crawler routes into this test binary for `discover()`.
  let _ = lindaflor::seo::sitemap::robots_txt;
  let _ = lindaflor::seo::sitemap::sitemap_xml;
  let req = Request::builder()
    .method(Method::GET)
    .uri(path)
    .header(header::HOST, HOST)
    .body(Body::empty())
    .expect("request");
  let response = router.handle(req).await;
  let status = response.status();
  let content_type = response
    .headers()
    .get(header::CONTENT_TYPE)
    .and_then(|v| v.to_str().ok())
    .unwrap_or("")
    .to_string();
  let body = String::from_utf8_lossy(&body_bytes(response).await).into_owned();
  (status, content_type, body)
}

#[tokio::test]
async fn robots_txt_disallows_private_areas_and_links_sitemap() {
  let (_pool, router) = pool_and_router().await;
  let (status, content_type, body) = get(&router, "/robots.txt").await;
  assert_eq!(status, StatusCode::OK, "{body}");
  assert!(content_type.starts_with("text/plain"), "{content_type}");
  assert!(body.starts_with("User-agent: *\n"), "{body}");
  for path in [
    "/admin",
    "/conta",
    "/checkout",
    "/carrinho",
    "/pedido",
    "/api",
    "/settings",
  ] {
    assert!(body.contains(&format!("Disallow: {path}\n")), "{body}");
  }
  assert!(
    body.contains(&format!("Sitemap: {}/sitemap.xml", expected_origin())),
    "{body}"
  );
}

#[tokio::test]
async fn sitemap_lists_active_products_and_sets_only() {
  let (pool, router) = pool_and_router().await;
  let tag = Uuid::now_v7().simple().to_string();
  let active_slug = format!("seo-ativo-{tag}");
  let inactive_slug = format!("seo-inativo-{tag}");
  for (slug, active) in [(&active_slug, true), (&inactive_slug, false)] {
    sqlx::query(
      "INSERT INTO products (id, name, slug, price_in_cents, category, active, featured, created_at, updated_at)
       VALUES ($1, 'Maiô SEO', $2, 12990, 'maio', $3, false, '2026-10-01 12:00:00', '2026-10-20 08:30:00')",
    )
    .bind(Uuid::now_v7())
    .bind(slug)
    .bind(active)
    .execute(&pool)
    .await
    .expect("insert product");
  }
  let active_set = format!("seo-set-ativo-{tag}");
  let inactive_set = format!("seo-set-inativo-{tag}");
  for (slug, active) in [(&active_set, true), (&inactive_set, false)] {
    sqlx::query(
      "INSERT INTO sets (id, name, slug, active, updated_at)
       VALUES ($1, 'Conjunto SEO', $2, $3, '2026-10-21 10:00:00+00')",
    )
    .bind(Uuid::now_v7())
    .bind(slug)
    .bind(active)
    .execute(&pool)
    .await
    .expect("insert set");
  }

  let (status, content_type, body) = get(&router, "/sitemap.xml").await;
  assert_eq!(status, StatusCode::OK, "{body}");
  assert!(
    content_type.starts_with("application/xml"),
    "{content_type}"
  );
  assert!(body.starts_with("<?xml"), "{body}");
  let origin = expected_origin();

  for loc in [
    format!("{origin}/"),
    format!("{origin}/produtos"),
    format!("{origin}/sets"),
    format!("{origin}/termos"),
    format!("{origin}/politica-privacidade"),
    format!("{origin}/trocas-devolucoes"),
    format!("{origin}/guia-de-medidas"),
    format!("{origin}/produtos?category=maio"),
  ] {
    assert!(body.contains(&format!("<loc>{loc}</loc>")), "{loc}\n{body}");
  }
  assert!(
    body.contains(&format!(
      "<loc>{origin}/produtos/{active_slug}</loc><lastmod>2026-10-20</lastmod>"
    )),
    "{body}"
  );
  assert!(
    body.contains(&format!(
      "<loc>{origin}/sets/{active_set}</loc><lastmod>2026-10-21</lastmod>"
    )),
    "{body}"
  );
  assert!(!body.contains(&inactive_slug), "{body}");
  assert!(!body.contains(&inactive_set), "{body}");
  assert!(!body.contains("/admin"), "{body}");
}
