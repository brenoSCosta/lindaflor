use lindaflor::auth::user::SessionUser;
use topcoat::{
  Result,
  context::Cx,
  cookie::{Cookies, cookies},
  icon::{icon, iconify::iconify_icon},
  router::{
    content::Form,
    error::{SeeOther, see_other},
    request::uri,
    route,
  },
  runtime::{Event, signal},
  view::{Child, StaticClass, View, attributes, class, component, view},
};

use crate::app::auth_helpers::optional_user;
use crate::components::avatar::{
  AvatarSize, avatar, avatar_fallback, avatar_image,
};
use crate::components::dropdown_menu::{
  dropdown_menu, dropdown_menu_label, dropdown_menu_separator,
  dropdown_menu_trigger,
};
use crate::components::sidebar::{
  SidebarCollapsible, SidebarMenuButtonSize, SidebarMenuButtonVariant, sidebar,
  sidebar_content, sidebar_footer, sidebar_group, sidebar_group_content,
  sidebar_group_label, sidebar_header, sidebar_inset, sidebar_menu,
  sidebar_menu_button, sidebar_menu_button_variants, sidebar_menu_item,
  sidebar_provider, sidebar_rail, sidebar_trigger,
};
use crate::theme::theme_toggle;
use lindaflor::auth::avatar::{self as auth_avatar, object_store};

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
  matches!(path, "/dashboard" | "/settings" | "/accept-invitation")
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
    let _ =
      lindaflor::auth::session_store::delete_by_token_hash(pool, &hash).await;
  }
  let redirect = body
    .and_then(|Form(b)| b.redirect)
    .filter(|p| p.starts_with('/') && !p.starts_with("//"))
    .unwrap_or_else(|| "/".to_string());
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
  let rail_toggle = toggle_sidebar.clone();
  let pathname = uri(cx).path().to_owned();
  let dashboard_active = path_active(&pathname, "/dashboard");
  let users_active = path_active(&pathname, "/admin/usuarios");
  let show_users =
    lindaflor::auth::service::is_admin(user.user.role.as_deref());
  let name = user.user.name.clone();
  let email = user.user.email.clone();
  let user_initials = initials(&name);
  let avatar_url = auth_avatar::resolve_avatar_url(
    &object_store(cx),
    user.user.image.as_deref(),
  )
  .await;
  let menu_avatar_url = avatar_url.clone();
  let menu_name = name.clone();
  let menu_email = email.clone();
  let trigger_name = name.clone();
  let trigger_initials = user_initials.clone();

  Ok(view! {
      sidebar_provider(
          sidebar(
              open: $(expanded.get()),
              collapsible: SidebarCollapsible::Icon,
              sheet_attrs: attributes! { aria-label="Menu lateral" },
              sidebar_header(
                  sidebar_menu(
                      sidebar_menu_item(
                          sidebar_menu_button(
                              href: Some("/"),
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
              )
              sidebar_content(
                  sidebar_group(
                      sidebar_group_label("Geral")
                      sidebar_group_content(
                          sidebar_menu(
                              sidebar_menu_item(
                                  sidebar_menu_button(
                                      href: Some("/dashboard"),
                                      active: dashboard_active,
                                      tooltip: Some("Painel"),
                                      icon(data: iconify_icon!("lucide:grid"))
                                      <span>"Painel"</span>
                                  )
                              )
                              if show_users {
                                  sidebar_menu_item(
                                      sidebar_menu_button(
                                          href: Some("/admin/usuarios"),
                                          active: users_active,
                                          tooltip: Some("Usuários"),
                                          icon(data: iconify_icon!("lucide:users"))
                                          <span>"Usuários"</span>
                                      )
                                  )
                              }
                          )
                      )
                  )
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
                                  dropdown_menu_label("Configurações")
                                  <a href="/settings?tab=profile" class=(class!(USER_MENU_ITEM, "hover:bg-foreground/5"))>
                                      icon(data: iconify_icon!("lucide:users"))
                                      "Perfil"
                                  </a>
                                  <a href="/settings?tab=account" class=(class!(USER_MENU_ITEM, "hover:bg-foreground/5"))>
                                      icon(data: iconify_icon!("lucide:badge-check"))
                                      "Conta"
                                  </a>
                                  <a href="/settings?tab=security" class=(class!(USER_MENU_ITEM, "hover:bg-foreground/5"))>
                                      icon(data: iconify_icon!("lucide:shield"))
                                      "Segurança"
                                  </a>
                                  dropdown_menu_separator()
                                  <form method="post" action="/logout">
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
                          aria-label="Alternar menu lateral"
                          title="Alternar menu lateral"
                          (toggle_sidebar)
                      },
                  )
                  <div class="ml-auto flex items-center gap-2">
                      theme_toggle()
                  </div>
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
  let logged_in = optional_user(cx).await?.is_some();

  Ok(view! {
      <div class="min-h-screen bg-background text-foreground">
          <div class="border-b border-border bg-background px-4 py-2 text-center text-[10px] tracking-widest text-muted-foreground uppercase">
              "Frete grátis acima de R$ 299 · Aracaju, SE"
          </div>
          <header class="sticky top-0 z-50 border-b border-border bg-background backdrop-blur">
              <div class="mx-auto grid h-20 max-w-7xl grid-cols-[1fr_auto_1fr] items-center gap-4 px-4 md:px-8">
                  <nav class="hidden items-center gap-6 md:flex">
                      <a href="/produtos" class="text-[10px] tracking-wider uppercase transition-colors hover:text-primary">"Catálogo"</a>
                      <a href="/colecoes" class="text-[10px] tracking-wider uppercase transition-colors hover:text-primary">"Coleções"</a>
                      <a href="/#sobre" class="text-[10px] tracking-wider uppercase transition-colors hover:text-primary">"Sobre"</a>
                  </nav>
                  <a href="/" class="text-center">
                      <span class="font-serif text-2xl leading-none text-foreground">"Linda Flor"</span>
                      <span class="block text-[10px] tracking-[0.24em] text-primary uppercase">"Moda Praia"</span>
                  </a>
                  <div class="flex items-center justify-end gap-2">
                      theme_toggle()
                      if logged_in {
                          <a href="/conta" class="text-[10px] tracking-wider uppercase transition-colors hover:text-primary">
                              "Conta"
                          </a>
                      } else {
                          <a href="/login" class="text-[10px] tracking-wider uppercase transition-colors hover:text-primary">
                              "Entrar"
                          </a>
                      }
                      <a href="/carrinho" class="text-[10px] tracking-wider uppercase transition-colors hover:text-primary">
                          "Carrinho"
                      </a>
                  </div>
              </div>
          </header>
          <main>(child)</main>
          <footer class="border-t border-border bg-background">
              <div class="mx-auto grid max-w-7xl gap-10 px-4 py-16 md:grid-cols-4 md:px-8">
                  <div class="space-y-3 md:col-span-2">
                      <p class="font-serif text-2xl text-foreground">"Linda Flor"</p>
                      <p class="max-w-md text-sm leading-relaxed text-muted-foreground">
                          "Moda praia feminina com qualidade e atendimento próximo. Da praia de Aracaju para você brilhar em qualquer verão."
                      </p>
                      <p class="text-[10px] tracking-wider text-primary uppercase">"@biquinislindaflor"</p>
                  </div>
                  <div class="space-y-3">
                      <h3 class="text-[10px] tracking-widest uppercase">"Loja"</h3>
                      <div class="flex flex-col gap-2 text-sm text-muted-foreground">
                          <a href="/produtos" class="transition-colors hover:text-foreground">"Catálogo"</a>
                          <a href="/colecoes" class="transition-colors hover:text-foreground">"Coleções"</a>
                          <a href="/produtos?category=biquini" class="transition-colors hover:text-foreground">"Biquínis"</a>
                          <a href="/produtos?category=maio" class="transition-colors hover:text-foreground">"Maiôs"</a>
                      </div>
                  </div>
                  <div class="space-y-3">
                      <h3 class="text-[10px] tracking-widest uppercase">"Contato"</h3>
                      <div class="space-y-2 text-sm text-muted-foreground">
                          <a href="https://wa.me/5579998165115" target="_blank" rel="noreferrer" class="transition-colors hover:text-foreground">"(79) 99816-5115"</a>
                          <p>"Rua Capitão Isaias Alves de Souza, 1100 · Aracaju, SE"</p>
                      </div>
                  </div>
              </div>
              <div class="border-t border-border px-4 py-5 text-center text-[10px] tracking-wider text-muted-foreground uppercase">
                  "© 2026 Linda Flor Moda Praia"
              </div>
          </footer>
      </div>
  })
}
