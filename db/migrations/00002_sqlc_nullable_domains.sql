-- +goose Up
-- Nullable marker domains for sqlc code generation.
--
-- sqlc cannot infer that a computed expression (scalar subquery, CASE,
-- aggregate, column of a LEFT JOIN subquery) may be NULL: it types it as a
-- non-pointer Go value and the scan fails at runtime on NULL. Casting such
-- an expression to one of these domains (`(SELECT ...)::null_text`) makes
-- sqlc emit a pointer (see the overrides in sqlc.yaml). The domains carry
-- no constraint and Postgres reports the base type on the wire, so pgx
-- decodes them like the base type.
CREATE DOMAIN null_text AS text;
CREATE DOMAIN null_int4 AS integer;
CREATE DOMAIN null_int8 AS bigint;
CREATE DOMAIN null_bool AS boolean;
CREATE DOMAIN null_uuid AS uuid;
CREATE DOMAIN null_timestamptz AS timestamptz;
