use http::{HeaderValue, header::CONTENT_TYPE};
use topcoat::{
  Result,
  context::Cx,
  router::{error::unauthorized, request, response::IntoResponse, route},
};

use super::metrics::format_prometheus;
use super::metrics_auth::metrics_request_is_authorized;

#[route(GET "/metrics")]
pub async fn metrics(cx: &Cx) -> Result<impl IntoResponse> {
  let authorization = request::headers(cx)
    .get(http::header::AUTHORIZATION)
    .and_then(|value| value.to_str().ok());
  if !metrics_request_is_authorized(
    authorization,
    crate::config::metrics_token().as_deref(),
  ) {
    return Err(unauthorized().into());
  }
  Ok((
    [(
      CONTENT_TYPE,
      HeaderValue::from_static("text/plain; version=0.0.4; charset=utf-8"),
    )],
    format_prometheus(),
  ))
}
