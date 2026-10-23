-- Owner: wave0 / wave1-B (platform: health, readiness).

-- name: Ping :one
SELECT 1::int AS ok;

-- name: VariantLiveAvailable :one
-- Sum of live (sellable) units of a variant across warehouses; see the
-- inventory_live_available() SQL function in the schema.
SELECT COALESCE(SUM(live_available), 0)::bigint AS available
FROM inventory_live
WHERE variant_id = $1;
