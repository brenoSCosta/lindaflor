-- Generic audit events (replaces admin_audit_log; nothing deployed).
--
-- Append-only trail for privileged mutations. `entity_id` deliberately has
-- NO foreign key so the row survives `remove_user` (DELETE FROM users) with
-- the removed id preserved. `entity_id` is NOT NULL: every event targets a
-- concrete entity (a removed user still has a known id); there is simply no
-- FK enforcing liveness since the entity may be deleted.
-- `actor_user_id` REFERENCES users(id) ON DELETE SET NULL (nullable) so the
-- event survives actor deletion.
-- NOTE: spec draft misspelled this column `occured_at`; the correct
-- spelling `occurred_at` is used here.

CREATE TABLE IF NOT EXISTS audit_events (
    id uuid PRIMARY KEY NOT NULL DEFAULT gen_random_uuid(),
    aggregate_type text NOT NULL,
    aggregate_id text NOT NULL,
    entity_type text NOT NULL,
    entity_id uuid NOT NULL,
    actor_user_id uuid,
    actor_name text,
    metadata jsonb NOT NULL DEFAULT '{}',
    occurred_at timestamptz NOT NULL DEFAULT now(),
    created_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT audit_events_actor_users_id_fk FOREIGN KEY (actor_user_id) REFERENCES users(id) ON DELETE SET NULL ON UPDATE no action
);

CREATE INDEX IF NOT EXISTS audit_events_aggregate_idx ON audit_events (aggregate_type, aggregate_id, occurred_at);
CREATE INDEX IF NOT EXISTS audit_events_entity_idx ON audit_events (entity_type, entity_id);
CREATE INDEX IF NOT EXISTS audit_events_actor_idx ON audit_events (actor_user_id);
