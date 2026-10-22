use crate::auth::user::SessionUser;
use sqlx::PgPool;
use topcoat::{
  Result,
  context::{Cx, app_context},
  cookie::{Cookies, cookies},
  icon::{icon, iconify::iconify_icon},
  router::{
    content::Form,
    error::{SeeOther, see_other},
    href,
    request::uri,
    route,
  },
  runtime::{Event, signal},
  view::{Child, StaticClass, View, attributes, class, component, view},
};

use crate::app::store::cart::cart_item_count;
use crate::app::store::cart_sheet::cart_sheet;
use crate::app::store::shell::{
  announcement::announcement_bar, footer::footer, header::header,
};
use crate::app::utils::{object_store, resolve_storage_url};
use crate::components::avatar::{
  AvatarSize, avatar, avatar_fallback, avatar_image,
};
use crate::components::button::{ButtonSize, ButtonVariant, button};
use crate::components::dropdown_menu::{
  dropdown_menu, dropdown_menu_label, dropdown_menu_separator,
  dropdown_menu_trigger,
};
use crate::components::sheet::{sheet, sheet_content};
use crate::components::sidebar::{
  SidebarCollapsible, SidebarMenuButtonSize, SidebarMenuButtonVariant, sidebar,
  sidebar_content, sidebar_footer, sidebar_group, sidebar_group_content,
  sidebar_group_label, sidebar_header, sidebar_inset, sidebar_menu,
  sidebar_menu_button, sidebar_menu_button_variants, sidebar_menu_item,
  sidebar_provider, sidebar_rail, sidebar_trigger,
};

pub const SIDEBAR_COOKIE: &str = "sidebar_state";

/// Cookie lifetime for the sidebar preference: seven days.
const SIDEBAR_COOKIE_MAX_AGE: &str = "604800";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SidebarState {
  Expanded,
  Collapsed,
}

impl SidebarState {
  pub fn is_collapsed(self) -> bool {
    matches!(self, Self::Collapsed)
  }
}

pub fn read_sidebar_state(cx: &Cx) -> SidebarState {
  match cookies(cx).get(SIDEBAR_COOKIE) {
    Some(cookie) if cookie.value() == "collapsed" => SidebarState::Collapsed,
    _ => SidebarState::Expanded,
  }
}

fn path_active(pathname: &str, to: &str) -> bool {
  pathname == to || pathname == format!("{to}/")
}

fn path_prefix(pathname: &str, prefix: &str) -> bool {
  pathname == prefix
    || pathname == format!("{prefix}/")
    || pathname.starts_with(&format!("{prefix}/"))
}

const USER_MENU_ITEM: StaticClass = class!(
  "flex w-full cursor-pointer items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm \
     whitespace-nowrap outline-none focus-visible:bg-foreground/5 active:bg-foreground/10 \
     [&>svg]:pointer-events-none [&>svg]:size-4 [&>svg]:shrink-0",
);

fn initials(name: &str) -> String {
  name
    .split_whitespace()
    .filter_map(|w| w.chars().next())
    .take(2)
    .collect::<String>()
    .to_uppercase()
}

/// Whether this request path should use the authenticated app chrome + sidebar.
pub fn is_app_shell_path(path: &str) -> bool {
  matches!(path, "/dashboard" | "/accept-invitation")
    || path == "/settings"
    || path.starts_with("/settings/")
    || path.starts_with("/conta")
    || path.starts_with("/admin")
}

#[derive(serde::Deserialize)]
struct SignOutForm {
  redirect: Option<String>,
}

#[route(POST "/logout")]
pub async fn logout_page(
  cx: &Cx,
  body: Option<Form<SignOutForm>>,
) -> Result<SeeOther> {
  let pool = topcoat::context::app_context::<sqlx::PgPool>(cx);
  if let Some(hash) = topcoat::session::stop(cx).await? {
    let _ = crate::auth::session_store::delete_by_token_hash(pool, &hash).await;
  }
  crate::app::store::cart::clear_cart_token(cx);
  let redirect = body
    .and_then(|Form(b)| b.redirect)
    .and_then(|p| crate::redirect::safe_local_redirect(&p))
    .unwrap_or_else(|| href!(crate::app::page).resolve(cx));
  Ok(see_other(redirect))
}

/// Authenticated chrome: brand, navigation, and account links in the sidebar primitive.
#[component]
pub async fn app_shell(
  cx: &Cx,
  user: SessionUser,
  #[default] child: Child<'_>,
) -> Result<impl View> {
  let expanded = signal(cx, || !read_sidebar_state(cx).is_collapsed());
  let toggle_sidebar = attributes! {
      @click=$(|_e: Event| {
          expanded.toggle();
          let state = if expanded.get() { "expanded" } else { "collapsed" };
          let max_age = SIDEBAR_COOKIE_MAX_AGE;
          raw!(
              r#"document.cookie = "sidebar_state=" + String(${state}) + "; Path=/; Max-Age=" + String(${max_age});"#,
              {
                  let _ = (state, max_age);
              }
          );
      })
  };
  let close_sidebar = attributes! {
      @click=$(|_e: Event| {
          expanded.set(false);
          let state = "collapsed";
          let max_age = SIDEBAR_COOKIE_MAX_AGE;
          raw!(
              r#"document.cookie = "sidebar_state=" + String(${state}) + "; Path=/; Max-Age=" + String(${max_age});"#,
              {
                  let _ = (state, max_age);
              }
          );
      })
  };
  let rail_toggle = toggle_sidebar.clone();
  let pathname = uri(cx).path().to_owned();
  let dashboard_active = path_active(&pathname, "/dashboard");
  let show_admin = crate::auth::service::is_admin(user.user.role.as_deref());
  let admin_active = path_active(&pathname, "/admin");
  let users_active = path_prefix(&pathname, "/admin/usuarios");
  let products_active = path_prefix(&pathname, "/admin/produtos");
  let sets_active = path_prefix(&pathname, "/admin/sets");
  let orders_active = path_prefix(&pathname, "/admin/pedidos");
  let stock_active = path_prefix(&pathname, "/admin/estoque");
  let settings_active = path_prefix(&pathname, "/admin/configuracoes");
  let coupons_active = path_prefix(&pathname, "/admin/cupons");
  let name = user.user.name.clone();
  let email = user.user.email.clone();
  let user_initials = initials(&name);
  let avatar_url =
    resolve_storage_url(&object_store(cx), user.user.image.as_deref()).await;
  let menu_avatar_url = avatar_url.clone();
  let menu_name = name.clone();
  let menu_email = email.clone();
  let trigger_name = name.clone();
  let trigger_initials = user_initials.clone();
  let home_href = href!(crate::app::page).resolve(cx);
  let dashboard_href = href!(crate::app::dashboard::page).resolve(cx);
  let admin_href = href!(crate::app::admin::page).resolve(cx);
  let users_href = href!(crate::app::admin::usuarios::page).resolve(cx);
  let products_href = href!(crate::app::admin::produtos::page).resolve(cx);
  let sets_href = href!(crate::app::admin::sets::page).resolve(cx);
  let orders_href = href!(crate::app::admin::pedidos::page).resolve(cx);
  let stock_href = href!(crate::app::admin::estoque::page).resolve(cx);
  let store_settings_href =
    href!(crate::app::admin::configuracoes::page).resolve(cx);
  let coupons_href = href!(crate::app::admin::cupons::page).resolve(cx);

  Ok(view! {
      sidebar_provider(
          sidebar(
              open: $(expanded.get()),
              mobile_open: $(expanded.get()),
              collapsible: SidebarCollapsible::Icon,
              sheet_attrs: attributes! {
                  id="app-sidebar"
                  aria-label="Menu lateral"
                  @keydown=$(|e: Event| {
                      if e.key == "Escape" {
                          expanded.set(false);
                          let state = "collapsed";
                          let max_age = SIDEBAR_COOKIE_MAX_AGE;
                          raw!(
                              r#"document.cookie = "sidebar_state=" + String(${state}) + "; Path=/; Max-Age=" + String(${max_age});"#,
                              {
                                  let _ = (state, max_age);
                              }
                          );
                      }
                  })
                  @click=$(|e: Event| {
                      if e.target.id == "app-sidebar" {
                          expanded.set(false);
                          let state = "collapsed";
                          let max_age = SIDEBAR_COOKIE_MAX_AGE;
                          raw!(
                              r#"document.cookie = "sidebar_state=" + String(${state}) + "; Path=/; Max-Age=" + String(${max_age});"#,
                              {
                                  let _ = (state, max_age);
                              }
                          );
                      }
                  })
              },
              sidebar_header(
                  <div class="flex items-center gap-2">
                      sidebar_menu(
                          attrs: attributes! { class="flex-1" },
                          sidebar_menu_item(
                              sidebar_menu_button(
                                  href: Some(home_href.as_str()),
                                  size: SidebarMenuButtonSize::Lg,
                                  tooltip: Some("Linda Flor"),
                                  <div class="flex aspect-square size-8 items-center justify-center rounded-lg bg-sidebar-primary text-sidebar-primary-foreground">
                                      icon(data: iconify_icon!("lucide:sun"))
                                  </div>
                                  <div class="grid flex-1 text-left text-sm leading-tight md:group-data-[collapsible=icon]/sidebar:hidden">
                                      <span class="truncate font-medium">"Linda Flor"</span>
                                      <span class="truncate text-xs text-muted-foreground">"Moda Praia"</span>
                                  </div>
                              )
                          )
                      )
                      button(
                          variant: ButtonVariant::Ghost,
                          size: ButtonSize::Icon,
                          attrs: attributes! {
                              type="button"
                              class="md:hidden"
                              aria-label="Fechar menu"
                              (close_sidebar)
                          },
                          icon(data: iconify_icon!("lucide:x"))
                      )
                  </div>
              )
              sidebar_content(
                  sidebar_group(
                      sidebar_group_label("Geral")
                      sidebar_group_content(
                          sidebar_menu(
                              sidebar_menu_item(
                                  sidebar_menu_button(
                                      href: Some(dashboard_href.as_str()),
                                      active: dashboard_active,
                                      tooltip: Some("Painel"),
                                      icon(data: iconify_icon!("lucide:grid"))
                                      <span>"Painel"</span>
                                  )
                              )
                          )
                      )
                  )
                  if show_admin {
                      sidebar_group(
                          sidebar_group_label("Admin")
                          sidebar_group_content(
                              sidebar_menu(
                                  sidebar_menu_item(
                                      sidebar_menu_button(
                                          href: Some(admin_href.as_str()),
                                          active: admin_active,
                                          tooltip: Some("Dashboard"),
                                          icon(data: iconify_icon!("lucide:layout-dashboard"))
                                          <span>"Dashboard"</span>
                                      )
                                  )
                                  sidebar_menu_item(
                                      sidebar_menu_button(
                                          href: Some(users_href.as_str()),
                                          active: users_active,
                                          tooltip: Some("Usuários"),
                                          icon(data: iconify_icon!("lucide:users"))
                                          <span>"Usuários"</span>
                                      )
                                  )
                                  sidebar_menu_item(
                                      sidebar_menu_button(
                                          href: Some(products_href.as_str()),
                                          active: products_active,
                                          tooltip: Some("Produtos"),
                                          icon(data: iconify_icon!("lucide:shirt"))
                                          <span>"Produtos"</span>
                                      )
                                  )
                                  sidebar_menu_item(
                                      sidebar_menu_button(
                                          href: Some(sets_href.as_str()),
                                          active: sets_active,
                                          tooltip: Some("Conjuntos"),
                                          icon(data: iconify_icon!("lucide:layers"))
                                          <span>"Conjuntos"</span>
                                      )
                                  )
                                  sidebar_menu_item(
                                      sidebar_menu_button(
                                          href: Some(orders_href.as_str()),
                                          active: orders_active,
                                          tooltip: Some("Pedidos"),
                                          icon(data: iconify_icon!("lucide:shopping-bag"))
                                          <span>"Pedidos"</span>
                                      )
                                  )
                                  sidebar_menu_item(
                                      sidebar_menu_button(
                                          href: Some(stock_href.as_str()),
                                          active: stock_active,
                                          tooltip: Some("Estoque"),
                                          icon(data: iconify_icon!("lucide:warehouse"))
                                          <span>"Estoque"</span>
                                      )
                                  )
                                  sidebar_menu_item(
                                      sidebar_menu_button(
                                          href: Some(store_settings_href.as_str()),
                                          active: settings_active,
                                          tooltip: Some("Configurações"),
                                          icon(data: iconify_icon!("lucide:settings"))
                                          <span>"Configurações"</span>
                                      )
                                  )
                                  sidebar_menu_item(
                                      sidebar_menu_button(
                                          href: Some(coupons_href.as_str()),
                                          active: coupons_active,
                                          tooltip: Some("Cupons"),
                                          icon(data: iconify_icon!("lucide:ticket-percent"))
                                          <span>"Cupons"</span>
                                      )
                                  )
                              )
                          )
                      )
                  }
              )
              sidebar_footer(
                  sidebar_menu(
                      sidebar_menu_item(
                          dropdown_menu(
                              attrs: attributes! { class="w-full" },
                              dropdown_menu_trigger(
                                  attrs: attributes! {
                                      title=(trigger_name)
                                      class=(class!(
                                          sidebar_menu_button_variants(
                                              SidebarMenuButtonVariant::Default,
                                              SidebarMenuButtonSize::Lg,
                                          ),
                                          "group-open:bg-sidebar-accent group-open:text-sidebar-accent-foreground",
                                      ))
                                  },
                                  avatar(
                                      size: AvatarSize::Sm,
                                      attrs: attributes! { class="rounded-lg!" },
                                      if let Some(url) = avatar_url {
                                          avatar_image(attrs: attributes! { src=(url) })
                                      }
                                      avatar_fallback((trigger_initials))
                                  )
                                  <span class="grid min-w-0 flex-1 text-left text-sm leading-tight md:group-data-[collapsible=icon]/sidebar:hidden">
                                      <span class="truncate font-medium">(name)</span>
                                      <span class="truncate text-xs">(email)</span>
                                  </span>
                                  icon(
                                      data: iconify_icon!("lucide:chevrons-up-down"),
                                      attrs: attributes! {
                                          class="ml-auto md:group-data-[collapsible=icon]/sidebar:hidden"
                                      },
                                  )
                              )
                              <div class="absolute bottom-full left-0 z-50 mb-1 min-w-56 rounded-lg border border-border bg-background p-1 text-foreground shadow-sm md:bottom-0 md:left-full md:mb-0 md:ml-1">
                                  <div class="flex items-center gap-2 px-1 py-1.5 text-left text-sm">
                                      avatar(
                                          size: AvatarSize::Sm,
                                          if let Some(url) = menu_avatar_url {
                                              avatar_image(attrs: attributes! { src=(url) })
                                          }
                                          avatar_fallback((user_initials))
                                      )
                                      <div class="grid min-w-0 flex-1 text-left text-sm leading-tight">
                                          <span class="truncate font-medium">(menu_name)</span>
                                          <span class="truncate text-xs">(menu_email)</span>
                                      </div>
                                  </div>
                                  dropdown_menu_separator()
                                  <a href=(href!(crate::app::favoritos::page)) class=(class!(USER_MENU_ITEM, "hover:bg-foreground/5"))>
                                      icon(data: iconify_icon!("lucide:heart"))
                                      "Favoritos"
                                  </a>
                                  <a href=(href!(crate::app::conta::pedidos::page)) class=(class!(USER_MENU_ITEM, "hover:bg-foreground/5"))>
                                      icon(data: iconify_icon!("lucide:shopping-bag"))
                                      "Pedidos"
                                  </a>
                                  <a href=(href!(crate::app::conta::enderecos::page)) class=(class!(USER_MENU_ITEM, "hover:bg-foreground/5"))>
                                      icon(data: iconify_icon!("lucide:map-pin"))
                                      "Endereços"
                                  </a>
                                  dropdown_menu_separator()
                                  dropdown_menu_label("Configurações")
                                  <a href=(href!(crate::app::settings::page).query([("tab", "profile")])) class=(class!(USER_MENU_ITEM, "hover:bg-foreground/5"))>
                                      icon(data: iconify_icon!("lucide:users"))
                                      "Perfil"
                                  </a>
                                  <a href=(href!(crate::app::settings::page).query([("tab", "account")])) class=(class!(USER_MENU_ITEM, "hover:bg-foreground/5"))>
                                      icon(data: iconify_icon!("lucide:badge-check"))
                                      "Conta"
                                  </a>
                                  <a href=(href!(crate::app::settings::page).query([("tab", "sessions")])) class=(class!(USER_MENU_ITEM, "hover:bg-foreground/5"))>
                                      icon(data: iconify_icon!("lucide:monitor"))
                                      "Sessões"
                                  </a>
                                  <a href=(href!(crate::app::settings::page).query([("tab", "security")])) class=(class!(USER_MENU_ITEM, "hover:bg-foreground/5"))>
                                      icon(data: iconify_icon!("lucide:shield"))
                                      "Segurança"
                                  </a>
                                  <a href=(href!(crate::app::settings::page).query([("tab", "linked-accounts")])) class=(class!(USER_MENU_ITEM, "hover:bg-foreground/5"))>
                                      icon(data: iconify_icon!("lucide:link"))
                                      "Contas vinculadas"
                                  </a>
                                  dropdown_menu_separator()
                                  <form method="post" action=(href!(logout_page))>
                                      <input type="hidden" name="redirect" value="/">
                                      <button
                                          type="submit"
                                          class=(class!(
                                              USER_MENU_ITEM,
                                              "text-destructive hover:bg-destructive/10 hover:text-destructive",
                                          ))
                                      >
                                          icon(data: iconify_icon!("lucide:log-out"))
                                          "Sair"
                                      </button>
                                  </form>
                              </div>
                          )
                      )
                  )
              )
              sidebar_rail(
                  open: $(expanded.get()),
                  attrs: rail_toggle,
              )
          )
          sidebar_inset(
              sidebar_header(
                  sidebar_trigger(
                      open: $(expanded.get()),
                      attrs: attributes! {
                          aria-controls="app-sidebar"
                          (toggle_sidebar)
                      },
                  )
              )
              <div class="flex-1">(child)</div>
          )
      )
  })
}

/// Public storefront chrome (header + footer) used outside the app sidebar.
#[component]
pub async fn storefront_shell(
  cx: &Cx,
  #[default] child: Child<'_>,
) -> Result<impl View> {
  let pool = app_context::<PgPool>(cx);
  let cart_count = cart_item_count(cx, pool).await?.max(0) as usize;
  let hide_footer = uri(cx).path() == "/login";
  let cart_open = signal(cx, || false);

  Ok(view! {
      <div class="flex min-h-screen flex-col bg-background text-foreground">
          announcement_bar()
          header(cart_count: cart_count, cart_open: cart_open.clone())
          <main class="flex flex-1 flex-col">(child)</main>
          sheet(
              open: $(cart_open.get()),
              attrs: attributes! {
                  id="cart-sheet"
                  aria-label="Carrinho"
                  @keydown=$(|e: Event| {
                      if e.key == "Escape" {
                          cart_open.set(false);
                      }
                  })
                  @click=$(|e: Event| {
                      if e.target.id == "cart-sheet" {
                          cart_open.set(false);
                      }
                  })
              },
              sheet_content(
                  attrs: attributes! { class="[&]:overflow-hidden" },
                  cart_sheet(open: cart_open.clone())
              )
          )
          if !hide_footer {
            footer()
          }
      </div>
  })
}
