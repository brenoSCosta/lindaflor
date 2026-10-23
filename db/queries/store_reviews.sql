-- Owner: store (product reviews, internal/store/reviews.go).
--
-- "Received" order of a user (delivered, or shipped at least 21 days ago):
--   o.user_id = @user_id AND (o.status = 'delivered'
--     OR (o.status = 'shipped' AND o.shipped_at <= now() - make_interval(days => 21)))

-- name: StoreEligibleOrder :one
SELECT o.id FROM orders o
WHERE o.user_id = sqlc.arg(user_id)::uuid
  AND (o.status = 'delivered'
       OR (o.status = 'shipped'
           AND o.shipped_at <= now() - make_interval(days => 21)))
  AND EXISTS (SELECT 1 FROM order_items oi
              JOIN product_variants v ON v.id = oi.variant_id
              WHERE oi.order_id = o.id AND v.product_id = sqlc.arg(product_id)::uuid)
ORDER BY o.created_at DESC LIMIT 1;

-- name: StoreOwnReviewStatus :one
SELECT status::text AS status FROM product_reviews WHERE user_id = $1 AND product_id = $2;

-- name: StoreInsertReview :one
INSERT INTO product_reviews (id, product_id, user_id, order_id, rating, title, body, fit)
VALUES (sqlc.arg(id), sqlc.arg(product_id), sqlc.arg(user_id), sqlc.arg(order_id), sqlc.arg(rating),
        sqlc.arg(title), sqlc.arg(body), sqlc.narg(fit)::text::review_fit)
ON CONFLICT (user_id, product_id) DO NOTHING
RETURNING id;

-- name: StoreRatingSummary :one
SELECT COALESCE(AVG(rating), 0)::float8 AS average,
       COUNT(*) AS count,
       COUNT(*) FILTER (WHERE fit = 'pequeno') AS fit_pequeno,
       COUNT(*) FILTER (WHERE fit = 'ideal') AS fit_ideal,
       COUNT(*) FILTER (WHERE fit = 'grande') AS fit_grande
FROM product_reviews
WHERE product_id = $1 AND status = 'approved';

-- name: StoreAllRatings :many
SELECT product_id, AVG(rating)::float8 AS average, COUNT(*) AS count
FROM product_reviews WHERE status = 'approved'
GROUP BY product_id;

-- name: StoreListApprovedReviews :many
SELECT r.id, r.rating, r.title, r.body, r.fit::text::null_text AS fit,
       u.name AS author_name, r.created_at
FROM product_reviews r
JOIN users u ON u.id = r.user_id
WHERE r.product_id = sqlc.arg(product_id) AND r.status = 'approved'
ORDER BY r.created_at DESC
LIMIT sqlc.arg(page_size)::bigint OFFSET sqlc.arg(page_offset)::bigint;

-- name: StoreReviewableForOrder :many
SELECT DISTINCT p.id AS product_id, p.name, p.slug
FROM orders o
JOIN order_items oi ON oi.order_id = o.id
JOIN product_variants v ON v.id = oi.variant_id
JOIN products p ON p.id = v.product_id
WHERE o.id = sqlc.arg(order_id)
  AND o.user_id = sqlc.arg(user_id)::uuid
  AND (o.status = 'delivered'
       OR (o.status = 'shipped'
           AND o.shipped_at <= now() - make_interval(days => 21)))
  AND p.active
  AND NOT EXISTS (SELECT 1 FROM product_reviews r
                  WHERE r.user_id = sqlc.arg(user_id)::uuid AND r.product_id = p.id)
ORDER BY p.name;

-- name: StoreReviewableOrders :many
SELECT DISTINCT o.id
FROM orders o
JOIN order_items oi ON oi.order_id = o.id
JOIN product_variants v ON v.id = oi.variant_id
JOIN products p ON p.id = v.product_id
WHERE o.user_id = sqlc.arg(user_id)::uuid
  AND (o.status = 'delivered'
       OR (o.status = 'shipped'
           AND o.shipped_at <= now() - make_interval(days => 21)))
  AND p.active
  AND NOT EXISTS (SELECT 1 FROM product_reviews r
                  WHERE r.user_id = sqlc.arg(user_id)::uuid AND r.product_id = p.id);

-- name: StoreAdminReviews :many
SELECT r.id, p.name AS product_name, p.slug AS product_slug,
       u.name AS author_name, u.email AS author_email,
       r.rating, r.title, r.body, r.fit::text::null_text AS fit,
       r.status::text AS status, r.created_at
FROM product_reviews r
JOIN products p ON p.id = r.product_id
JOIN users u ON u.id = r.user_id
ORDER BY (r.status = 'pending') DESC,
         CASE WHEN r.status = 'pending' THEN r.created_at END ASC,
         r.moderated_at DESC NULLS LAST, r.created_at DESC
LIMIT sqlc.arg(max_rows)::bigint;

-- name: StorePendingReviewCount :one
SELECT COUNT(*) FROM product_reviews WHERE status = 'pending';

-- name: StoreReviewLock :one
SELECT status::text AS status, product_id FROM product_reviews WHERE id = $1 FOR UPDATE;

-- name: StoreModerateReview :exec
UPDATE product_reviews
SET status = sqlc.arg(status)::review_status, moderated_at = now(), moderated_by = sqlc.arg(moderated_by)::uuid
WHERE id = sqlc.arg(id);
