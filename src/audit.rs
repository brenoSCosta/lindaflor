use sqlx::Postgres;
use uuid::Uuid;

/// Append-only privileged mutation trail (`audit_events`).
pub async fn record_audit_event<'e, E>(
  executor: E,
  aggregate_type: &str,
  aggregate_id: &str,
  entity_type: &str,
  entity_id: Uuid,
  actor_id: Option<Uuid>,
  actor_name: Option<&str>,
  metadata: serde_json::Value,
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
    aggregate_type,
    aggregate_id,
    entity_type,
    entity_id,
    actor_id,
    actor_name,
    metadata,
  )
  .execute(executor)
  .await?;
  Ok(())
}
