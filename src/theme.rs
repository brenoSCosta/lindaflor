use serde::Deserialize;
use topcoat::{
  Result,
  context::Cx,
  cookie::{Cookie, Cookies, cookies, time::Duration},
  icon::{icon, iconify::iconify_icon},
  router::{
    content::Form,
    error::{SeeOther, see_other},
    route,
  },
  view::{Attributes, View, attributes, class, component, view},
};

use crate::components::button::{ButtonSize, ButtonVariant, button};

pub const THEME_COOKIE: &str = "theme";

/// Cookie `max-age` in seconds — matches [`write_theme`] (`Duration::days(365)`).
const THEME_COOKIE_MAX_AGE_SECS: i64 = 365 * 24 * 60 * 60;

/// Blocking inline script for `<head>`: sync `dark` on `<html>` from the `theme`
/// cookie before paint (next-themes-style FOUC prevention). Default is dark.
pub const THEME_INIT_SCRIPT: &str = concat!(
  "<script>(function(){",
  "var m=document.cookie.match(/(?:^|;\\s*)theme=([^;]*)/);",
  "var t=m?decodeURIComponent(m[1]):'dark';",
  "var d=document.documentElement;",
  "if(t==='light')d.classList.remove('dark');",
  "else d.classList.add('dark');",
  "})();</script>"
);

/// Client-side toggle: flip `dark` on `<html>`, persist `theme` cookie, update labels.
/// Icons swap via CSS (`dark:`) — no navigation.
const THEME_TOGGLE_ONCLICK: &str = concat!(
  "(function(b){",
  "var r=document.documentElement;",
  "var dark=r.classList.toggle('dark');",
  "var t=dark?'dark':'light';",
  "document.cookie='theme='+t+'; path=/; max-age=",
  "31536000",
  "';",
  "var l=dark?'Ativar tema claro':'Ativar tema escuro';",
  "b.setAttribute('aria-label',l);",
  "b.setAttribute('title',l);",
  "})(this)"
);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Theme {
  Light,
  Dark,
}

impl Theme {
  pub fn as_str(self) -> &'static str {
    match self {
      Self::Light => "light",
      Self::Dark => "dark",
    }
  }

  pub fn toggled(self) -> Self {
    match self {
      Self::Light => Self::Dark,
      Self::Dark => Self::Light,
    }
  }

  pub fn is_dark(self) -> bool {
    matches!(self, Self::Dark)
  }
}

pub fn read_theme(cx: &Cx) -> Theme {
  match cookies(cx).get(THEME_COOKIE) {
    Some(cookie) if cookie.value() == "light" => Theme::Light,
    // Default matches the previous hardcoded dark layout.
    _ => Theme::Dark,
  }
}

pub fn write_theme(cx: &Cx, theme: Theme) {
  cookies(cx).add(
    Cookie::build((THEME_COOKIE, theme.as_str()))
      .path("/")
      .max_age(Duration::days(365))
      .build(),
  );
}

pub fn toggle_theme(cx: &Cx) -> Theme {
  let next = read_theme(cx).toggled();
  write_theme(cx, next);
  next
}

fn safe_redirect(target: Option<&str>) -> &str {
  match target {
    Some(path) if path.starts_with('/') && !path.starts_with("//") => path,
    _ => "/",
  }
}

#[derive(Deserialize)]
pub struct ThemeToggleInput {
  redirect: Option<String>,
}

/// Optional progressive-enhancement fallback (no-JS / direct POST). Default UX
/// uses the client-side [`theme_toggle`] and does not navigate.
#[route(POST "/theme")]
pub async fn toggle(
  cx: &Cx,
  Form(input): Form<ThemeToggleInput>,
) -> Result<SeeOther> {
  toggle_theme(cx);
  Ok(see_other(safe_redirect(input.redirect.as_deref())))
}

/// Icon button that toggles theme client-side (class + cookie, no reload).
#[component]
pub async fn theme_toggle(
  cx: &Cx,
  #[default] mut attrs: Attributes,
) -> Result<impl View> {
  let theme = read_theme(cx);
  let label = if theme.is_dark() {
    "Ativar tema claro"
  } else {
    "Ativar tema escuro"
  };

  // Compile-time check that the onclick max-age stays aligned with write_theme.
  const _: () = assert!(THEME_COOKIE_MAX_AGE_SECS == 31536000);

  Ok(view! {
      button(
          variant: ButtonVariant::Ghost,
          size: ButtonSize::Icon,
          attrs: attributes! {
              type="button"
              class=(class!("relative", attrs.remove("class")))
              aria-label=(label)
              title=(label)
              onclick=(THEME_TOGGLE_ONCLICK)
              (attrs)
          },
          // Sun when dark (switch to light); moon when light (switch to dark).
          icon(
              data: iconify_icon!("lucide:sun"),
              attrs: attributes! {
                  class="size-[1em] scale-0 rotate-90 transition-all dark:scale-100 dark:rotate-0"
              }
          )
          icon(
              data: iconify_icon!("lucide:moon"),
              attrs: attributes! {
                  class="absolute size-[1em] scale-100 rotate-0 transition-all dark:scale-0 dark:-rotate-90"
              }
          )
      )
  })
}
