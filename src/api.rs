use http::StatusCode;
use serde::Serialize;
use sqlx::PgPool;
use topcoat::{
  Result,
  context::{Cx, try_app_context},
  router::{content::Json, route},
};

#[derive(Clone, Serialize, utoipa::ToSchema)]
pub struct HealthResponse {
  pub status: &'static str,
}

/// Liveness check used by probes and OpenAPI examples.
///
/// Cheap by design: no backend I/O. Pair with `GET /api/readyz` for
/// dependency-aware readiness.
#[utoipa::path(
    get,
    path = "/api/health",
    tag = "system",
    responses(
        (status = 200, description = "Service is healthy", body = HealthResponse)
    )
)]
#[route(GET "/api/health")]
pub async fn health() -> Result<Json<HealthResponse>> {
  Ok(Json(HealthResponse { status: "ok" }))
}

#[derive(Clone, Serialize, utoipa::ToSchema)]
pub struct ReadyResponse {
  pub status: &'static str,
  pub database: &'static str,
  pub valkey: &'static str,
  pub storage: &'static str,
}

/// Readiness check: `SELECT 1` (Postgres), `PING` (Valkey), `HEAD` bucket
/// (S3) — each bounded by a 2s timeout. Returns `200` when every
/// *registered* backend passes and `503` when any of them is degraded.
///
/// Backends with no context registered (e.g. the API-only test router, or
/// an `Unavailable` object store) are reported as `skipped`, not failures.
#[utoipa::path(
    get,
    path = "/api/readyz",
    tag = "system",
    responses(
        (status = 200, description = "All backends ready", body = ReadyResponse),
        (status = 503, description = "One or more backends degraded", body = ReadyResponse)
    )
)]
#[route(GET "/api/readyz")]
pub async fn readyz(cx: &Cx) -> Result<(StatusCode, Json<ReadyResponse>)> {
  const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

  let pool = try_app_context::<PgPool>(cx).cloned();
  let database = match pool {
    None => "skipped",
    Some(pool) => {
      let ok = tokio::time::timeout(TIMEOUT, async {
        sqlx::query("SELECT 1").execute(&pool).await
      })
      .await
      .is_ok_and(|result| result.is_ok());
      if ok { "ok" } else { "degraded" }
    }
  };

  let valkey =
    match try_app_context::<redis::aio::MultiplexedConnection>(cx).cloned() {
      None => "skipped",
      Some(mut conn) => {
        let ok = tokio::time::timeout(TIMEOUT, async {
          redis::cmd("PING").query_async::<String>(&mut conn).await
        })
        .await
        .is_ok_and(|result| result.is_ok());
        if ok { "ok" } else { "degraded" }
      }
    };

  let storage =
    match try_app_context::<crate::storage::ObjectStore>(cx).cloned() {
      None => "skipped",
      Some(store) => {
        let ok = tokio::time::timeout(TIMEOUT, store.health_check())
          .await
          .is_ok_and(|result| result.is_ok());
        if ok { "ok" } else { "degraded" }
      }
    };

  let degraded = [database, valkey, storage].contains(&"degraded");
  let status = if degraded {
    StatusCode::SERVICE_UNAVAILABLE
  } else {
    StatusCode::OK
  };
  Ok((
    status,
    Json(ReadyResponse {
      status: if degraded { "degraded" } else { "ok" },
      database,
      valkey,
      storage,
    }),
  ))
}
