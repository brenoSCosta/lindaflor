use serde::Serialize;
use topcoat::{
  Result,
  router::{content::Json, route},
};

#[derive(Clone, Serialize, utoipa::ToSchema)]
pub struct HealthResponse {
  pub status: &'static str,
}

/// Liveness check used by probes and OpenAPI examples.
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
