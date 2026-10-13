use sqlx::Postgres;
use uuid::Uuid;

/// One row of the `audit_events` trail.
pub struct AuditEvent<'a> {
  pub aggregate_type: &'a str,
  pub aggregate_id: &'a str,
  pub entity_type: &'a str,
  pub entity_id: Uuid,
  pub actor_id: Option<Uuid>,
  pub actor_name: Option<&'a str>,
  pub metadata: serde_json::Value,
}

/// Append-only privileged mutation trail (`audit_events`).
pub async fn record_audit_event<'e, E>(
  executor: E,
  event: AuditEvent<'_>,
) -> Result<(), sqlx::Error>
where
  E: sqlx::Executor<'e, Database = Postgres>,
{
  sqlx::query!(
    r#"
        INSERT INTO audit_events
            (id, aggregate_type, aggregate_id, entity_type, entity_id,
             actor_user_id, actor_name, metadata)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        "#,
    Uuid::now_v7(),
    event.aggregate_type,
    event.aggregate_id,
    event.entity_type,
    event.entity_id,
    event.actor_id,
    event.actor_name,
    event.metadata,
  )
  .execute(executor)
  .await?;
  Ok(())
}
