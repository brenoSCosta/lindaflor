-- Owner: wave1-B (sitemap.xml).

-- name: SeoActiveCategories :many
SELECT DISTINCT category::text AS category FROM products WHERE active = true ORDER BY 1;

-- name: SeoActiveProducts :many
SELECT slug, to_char(updated_at, 'YYYY-MM-DD')::text AS lastmod
FROM products WHERE active = true
ORDER BY updated_at DESC, slug ASC;

-- name: SeoActiveSets :many
SELECT slug, to_char(updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD')::text AS lastmod
FROM sets WHERE active = true
ORDER BY updated_at DESC, slug ASC;
