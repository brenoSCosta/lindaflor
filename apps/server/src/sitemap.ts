import { env } from "@lindaflor/env/server";
import { listProductSlugsForSitemap } from "@lindaflor/core/commerce/products";

export async function buildSitemapXml() {
  const products = await listProductSlugsForSitemap();
  const base = env.WEB_ORIGIN.replace(/\/$/, "");
  const staticUrls = ["", "/produtos", "/colecoes", "/politica-privacidade", "/termos", "/trocas-devolucoes"];

  const urls = [
    ...staticUrls.map(
      (path) => `  <url><loc>${base}${path}</loc><changefreq>weekly</changefreq></url>`,
    ),
    ...products.map(
      (product) =>
        `  <url><loc>${base}/produtos/${product.slug}</loc><lastmod>${product.updated_at.toISOString().split("T")[0]}</lastmod><changefreq>weekly</changefreq></url>`,
    ),
  ];

  return `<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
${urls.join("\n")}
</urlset>`;
}
