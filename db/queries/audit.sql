-- Owner: wave0 (internal/audit).

-- name: InsertAuditEvent :exec
INSERT INTO audit_events
    (id, aggregate_type, aggregate_id, entity_type, entity_id, actor_user_id, actor_name, metadata)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8);

-- name: ListAuditEventsForEntity :many
SELECT * FROM audit_events
WHERE entity_type = $1 AND entity_id = $2
ORDER BY occurred_at, id;
