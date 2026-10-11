use serde::Deserialize;
use sqlx::PgPool;
use topcoat::{
  Result,
  context::{Cx, app_context, memoize},
  icon::{icon, iconify::iconify_icon},
  router::{content::Form, href, request, response::Response, route},
  view::{View, component, view},
};
use uuid::Uuid;

use crate::app::auth_helpers::optional_user;
use crate::app::store::queries::{ProductSummary, list_wishlist_products};
use crate::components::toast::{Toast, set_toast, toast_redirect};

/// Product ids the user has favorited, for marking hearts in a grid.
pub async fn wishlist_ids(pool: &PgPool, user_id: Uuid) -> Result<Vec<Uuid>> {
  Ok(
    sqlx::query_scalar(
      "SELECT product_id FROM wishlist_items WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?,
  )
}

/// Product ids the current user has favorited, loaded once per request so
/// every card in a grid shares one query. Empty for guests or on error.
#[memoize]
pub async fn liked_product_ids(cx: &Cx) -> Vec<Uuid> {
  let Ok(Some(user)) = optional_user(cx).await else {
    return Vec::new();
  };
  wishlist_ids(app_context::<PgPool>(cx), user.user.id)
    .await
    .unwrap_or_default()
}

/// Wishlist products with storage keys resolved to image URLs.
pub async fn get_wishlist(
  cx: &Cx,
  pool: &PgPool,
  user_id: Uuid,
) -> Result<Vec<ProductSummary>> {
  let products = list_wishlist_products(pool, user_id).await?;
  let store = crate::app::utils::object_store(cx);
  Ok(
    crate::app::admin::produtos::resolve_summary_images(&store, products).await,
  )
}

#[derive(Deserialize)]
struct ToggleForm {
  product_id: String,
  next: Option<String>,
}

/// Same-site path to return to; anything else falls back to the wishlist.
fn return_path(next: Option<&str>) -> Option<String> {
  let next = next?.trim();
  (next.starts_with('/') && !next.starts_with("//") && !next.contains('\\'))
    .then(|| next.to_string())
}

/// Path + query of a same-origin Referer, used when the form sends no `next`
/// (e.g. cards rendered inside a shard, where the request URI is the shard's).
fn referer_path(cx: &Cx) -> Option<String> {
  let headers = request::headers(cx);
  let referer = headers.get(http::header::REFERER)?.to_str().ok()?;
  let host = headers.get(http::header::HOST)?.to_str().ok()?;
  let rest = referer
    .strip_prefix("https://")
    .or_else(|| referer.strip_prefix("http://"))?;
  let path = rest.strip_prefix(host)?;
  return_path(Some(path))
}

#[route(POST "/favoritos/alternar")]
async fn toggle_wishlist(
  cx: &Cx,
  Form(body): Form<ToggleForm>,
) -> Result<Response> {
  let back = return_path(body.next.as_deref())
    .or_else(|| referer_path(cx))
    .unwrap_or_else(|| href!(crate::app::favoritos::page).resolve(cx));
  let Some(user) = optional_user(cx).await? else {
    let login = href!(crate::app::login::page)
      .query([("next", back.as_str())])
      .resolve(cx);
    return toast_redirect(cx, login);
  };
  let Ok(product_id) = Uuid::parse_str(body.product_id.trim()) else {
    set_toast(cx, Toast::error("Produto inválido."));
    return toast_redirect(cx, back);
  };
  let pool = app_context::<PgPool>(cx);

  let removed = sqlx::query(
    "DELETE FROM wishlist_items WHERE user_id = $1 AND product_id = $2",
  )
  .bind(user.user.id)
  .bind(product_id)
  .execute(pool)
  .await?
  .rows_affected()
    > 0;
  if removed {
    set_toast(cx, Toast::success("Removido dos favoritos."));
  } else {
    // Only active products can be favorited; unknown ids insert nothing.
    sqlx::query(
      "INSERT INTO wishlist_items (user_id, product_id)
       SELECT $1, id FROM products WHERE id = $2 AND active = true
       ON CONFLICT DO NOTHING",
    )
    .bind(user.user.id)
    .bind(product_id)
    .execute(pool)
    .await?;
    set_toast(cx, Toast::success("Adicionado aos favoritos."));
  }
  toast_redirect(cx, back)
}

/// Heart toggle. `next` is the page to come back to after toggling; pass an
/// empty string to fall back to the Referer.
#[component]
pub async fn wishlist_button(
  product_id: String,
  liked: bool,
  next: String,
) -> Result<impl View> {
  let label = if liked {
    "Remover dos favoritos"
  } else {
    "Adicionar aos favoritos"
  };
  Ok(view! {
      <form method="post" action=(href!(toggle_wishlist)) data-toast-promise="" data-toast-loading="Salvando…">
          <input type="hidden" name="product_id" value=(product_id)>
          <input type="hidden" name="next" value=(next)>
          <button
              type="submit"
              title=(label)
              aria-label=(label)
              aria-pressed=(if liked { "true" } else { "false" })
              class=(if liked {
                  "rounded-full bg-background/90 p-2 text-rose-600 shadow-sm transition hover:scale-105 [&_svg]:fill-current"
              } else {
                  "rounded-full bg-background/90 p-2 text-foreground shadow-sm transition hover:scale-105 hover:text-rose-600"
              })
          >
              icon(data: iconify_icon!("lucide:heart"), label: label)
          </button>
      </form>
  })
}

#[cfg(test)]
mod tests {
  use super::return_path;

  #[test]
  fn return_path_only_allows_same_site_paths() {
    assert_eq!(
      return_path(Some("/produtos?x=1")),
      Some("/produtos?x=1".into())
    );
    assert_eq!(return_path(Some("//evil.example")), None);
    assert_eq!(return_path(Some("/\\evil.example")), None);
    assert_eq!(return_path(Some("https://evil.example")), None);
    assert_eq!(return_path(None), None);
  }
}
