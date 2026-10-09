use uuid::Uuid;

pub const AUTH_ROUTE_LABEL: &str = "/api/auth/*";
pub const UNKNOWN_ROUTE_LABEL: &str = "other";

const KNOWN_TEMPLATES: &[&str] = &[
  "/",
  "/admin",
  "/admin/configuracoes",
  "/admin/cupons",
  "/admin/cupons/assign",
  "/admin/cupons/create",
  "/admin/cupons/delete",
  "/admin/cupons/toggle",
  "/admin/cupons/unassign",
  "/admin/estoque",
  "/admin/pedidos",
  "/admin/pedidos/{id}",
  "/admin/produtos",
  "/admin/produtos/create",
  "/admin/produtos/novo",
  "/admin/produtos/update",
  "/admin/produtos/{id}",
  "/admin/usuarios",
  "/admin/usuarios/atuar",
  "/admin/usuarios/banir",
  "/admin/usuarios/desbanir",
  "/admin/usuarios/nome",
  "/admin/usuarios/papel",
  "/admin/usuarios/remover",
  "/admin/usuarios/sessoes/revogar",
  "/admin/usuarios/sessoes/revogar-todas",
  "/api/health",
  "/carrinho",
  "/check-email",
  "/check-email/resend",
  "/checkout",
  "/colecoes",
  "/colecoes/{slug}",
  "/conta/pedidos",
  "/dashboard",
  "/forgot-password",
  "/forgot-password/sent",
  "/login",
  "/login/google",
  "/logout",
  "/pedido",
  "/pedido/status-proc",
  "/pedido/{id}",
  "/politica-privacidade",
  "/produtos",
  "/produtos/{slug}",
  "/reset-password",
  "/settings",
  "/settings/avatar",
  "/signup",
  "/stop-impersonating",
  "/termos",
  "/theme",
  "/trocas-devolucoes",
  "/two-factor",
  "/verify-email",
  "/verify-email/resend",
];

pub fn normalize_route_label(pathname: &str) -> String {
  if pathname == "/" {
    return "/".to_string();
  }
  if pathname == "/api/auth" || pathname.starts_with("/api/auth/") {
    return AUTH_ROUTE_LABEL.to_string();
  }
  if looks_like_asset(pathname) {
    return UNKNOWN_ROUTE_LABEL.to_string();
  }

  let templated = templatize(pathname);
  if is_known(&templated) {
    templated
  } else {
    UNKNOWN_ROUTE_LABEL.to_string()
  }
}

fn is_known(templated: &str) -> bool {
  KNOWN_TEMPLATES.binary_search(&templated).is_ok()
}

fn looks_like_asset(pathname: &str) -> bool {
  let last = pathname.rsplit('/').next().unwrap_or("");
  let Some((_, ext)) = last.rsplit_once('.') else {
    return false;
  };
  !ext.is_empty() && ext.chars().all(|c| c.is_ascii_alphanumeric())
}

fn templatize(pathname: &str) -> String {
  let mut out = String::new();
  let mut prev: &str = "";
  for segment in pathname.split('/').filter(|s| !s.is_empty()) {
    out.push('/');
    if looks_like_uuid(segment) || is_numeric_id(segment) {
      out.push_str("{id}");
    } else if matches!(prev, "produtos" | "colecoes") {
      out.push_str("{slug}");
    } else {
      out.push_str(segment);
    }
    prev = segment;
  }
  if out.is_empty() { "/".to_string() } else { out }
}

fn looks_like_uuid(segment: &str) -> bool {
  Uuid::parse_str(segment).is_ok() || {
    let b = segment.as_bytes();
    b.len() == 36
      && b[8] == b'-'
      && b[13] == b'-'
      && b[18] == b'-'
      && b[23] == b'-'
      && b
        .iter()
        .enumerate()
        .all(|(i, c)| matches!(i, 8 | 13 | 18 | 23) || c.is_ascii_hexdigit())
  }
}

fn is_numeric_id(segment: &str) -> bool {
  !segment.is_empty()
    && segment.len() <= 20
    && segment.bytes().all(|b| b.is_ascii_digit())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn known_templates_are_sorted() {
    let mut sorted = KNOWN_TEMPLATES.to_vec();
    sorted.sort_unstable();
    assert_eq!(KNOWN_TEMPLATES, sorted.as_slice());
  }

  #[test]
  fn keeps_root() {
    assert_eq!(normalize_route_label("/"), "/");
  }

  #[test]
  fn collapses_auth() {
    assert_eq!(
      normalize_route_label("/api/auth/sign-in/email"),
      AUTH_ROUTE_LABEL
    );
    assert_eq!(
      normalize_route_label("/api/auth/get-session"),
      AUTH_ROUTE_LABEL
    );
  }

  #[test]
  fn substitutes_dynamic_segments() {
    assert_eq!(
      normalize_route_label("/pedido/0199c1a0-0000-7000-8000-000000000001"),
      "/pedido/{id}"
    );
    assert_eq!(
      normalize_route_label("/produtos/rosa-vermelha"),
      "/produtos/{slug}"
    );
    assert_eq!(
      normalize_route_label("/admin/pedidos/abc"),
      UNKNOWN_ROUTE_LABEL
    );
    assert_eq!(
      normalize_route_label("/admin/pedidos/42"),
      "/admin/pedidos/{id}"
    );
  }

  #[test]
  fn collapses_unknown_and_assets() {
    assert_eq!(normalize_route_label("/wp-admin.php"), UNKNOWN_ROUTE_LABEL);
    assert_eq!(
      normalize_route_label("/some/unknown/deep/path"),
      UNKNOWN_ROUTE_LABEL
    );
    assert_eq!(normalize_route_label("/favicon.ico"), UNKNOWN_ROUTE_LABEL);
  }

  #[test]
  fn keeps_closed_api_and_pages() {
    assert_eq!(normalize_route_label("/api/health"), "/api/health");
    assert_eq!(normalize_route_label("/login"), "/login");
    assert_eq!(normalize_route_label("/checkout"), "/checkout");
  }
}
