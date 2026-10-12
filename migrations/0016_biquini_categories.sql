-- Consistent `biquini_*` prefix for the bikini-piece categories.
-- RENAME VALUE is metadata-only (enum values are stored as OIDs),
-- so existing rows keep working with no data rewrite.
ALTER TYPE product_category RENAME VALUE 'top_biquini' TO 'biquini_top';
ALTER TYPE product_category RENAME VALUE 'calcinha_biquini' TO 'biquini_bottom';
