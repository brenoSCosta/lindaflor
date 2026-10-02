use std::{
  collections::HashMap,
  sync::{Mutex, OnceLock, PoisonError},
};

use serde::{Deserialize, Serialize};
use topcoat::{
  Result,
  asset::{Asset, asset},
  context::{Cx, CxId},
  cookie::{Cookie, Cookies, SameSite, cookies, time::Duration},
  icon::{icon, iconify::iconify_icon},
  router::{
    HeaderName, HeaderValue, StatusCode,
    error::see_other,
    header::LOCATION,
    request::{headers, uri},
    response::{IntoResponse, Response, response_headers},
  },
  view::{View, attributes, component, view},
};

const TOAST_COOKIE: &str = "toast";
const TOAST_MAX_CHARS: usize = 180;
const MAX_TOASTS: usize = 5;
const COOKIE_BUDGET: usize = 3500;

/// Content-hashed URL for the toaster script.
const TOAST_SCRIPT: Asset = asset!("assets/toast.js");

fn default_dismissible() -> bool {
  true
}

fn is_false(value: &bool) -> bool {
  !*value
}

/// Visual tone of a toast. `default` has no icon.
#[derive(
  Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum ToastType {
  #[default]
  Default,
  Success,
  Info,
  Warning,
  Error,
  Loading,
}

impl ToastType {
  fn as_str(self) -> &'static str {
    match self {
      Self::Default => "default",
      Self::Success => "success",
      Self::Info => "info",
      Self::Warning => "warning",
      Self::Error => "error",
      Self::Loading => "loading",
    }
  }
}

/// A link rendered at the end of a toast.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToastLink {
  pub label: String,
  pub href: String,
}

/// One queued toast. `duration` of `Some(0)` stays until dismissed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Toast {
  #[serde(rename = "type")]
  pub kind: ToastType,
  pub title: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub description: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub duration: Option<u32>,
  #[serde(default = "default_dismissible")]
  pub dismissible: bool,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub action: Option<ToastLink>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub cancel: Option<ToastLink>,
  /// Already visible before this page loaded, so the enter animation must not
  /// play again. Set when a promise toast is carried across a navigation.
  #[serde(default, skip_serializing_if = "is_false")]
  pub shown: bool,
}

impl Toast {
  fn new(kind: ToastType, title: impl Into<String>) -> Self {
    Self {
      kind,
      title: title.into(),
      description: None,
      duration: None,
      dismissible: true,
      action: None,
      cancel: None,
      shown: false,
    }
  }

  pub fn plain(title: impl Into<String>) -> Self {
    Self::new(ToastType::Default, title)
  }

  pub fn success(title: impl Into<String>) -> Self {
    Self::new(ToastType::Success, title)
  }

  pub fn info(title: impl Into<String>) -> Self {
    Self::new(ToastType::Info, title)
  }

  pub fn warning(title: impl Into<String>) -> Self {
    Self::new(ToastType::Warning, title)
  }

  pub fn error(title: impl Into<String>) -> Self {
    Self::new(ToastType::Error, title)
  }

  pub fn loading(title: impl Into<String>) -> Self {
    Self::new(ToastType::Loading, title).duration(0)
  }

  pub fn description(mut self, description: impl Into<String>) -> Self {
    self.description = Some(description.into());
    self
  }

  /// `0` keeps the toast until the visitor dismisses it.
  pub fn duration(mut self, duration: u32) -> Self {
    self.duration = Some(duration);
    self
  }

  pub fn dismissible(mut self, dismissible: bool) -> Self {
    self.dismissible = dismissible;
    self
  }

  pub fn action(
    mut self,
    label: impl Into<String>,
    href: impl Into<String>,
  ) -> Self {
    self.action = Some(ToastLink {
      label: label.into(),
      href: href.into(),
    });
    self
  }

  pub fn cancel(
    mut self,
    label: impl Into<String>,
    href: impl Into<String>,
  ) -> Self {
    self.cancel = Some(ToastLink {
      label: label.into(),
      href: href.into(),
    });
    self
  }
}

/// Where the toaster sits in the viewport.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ToastPosition {
  TopLeft,
  TopCenter,
  TopRight,
  BottomLeft,
  BottomCenter,
  #[default]
  BottomRight,
}

impl ToastPosition {
  fn x(self) -> &'static str {
    match self {
      Self::TopLeft | Self::BottomLeft => "left",
      Self::TopCenter | Self::BottomCenter => "center",
      Self::TopRight | Self::BottomRight => "right",
    }
  }

  fn y(self) -> &'static str {
    match self {
      Self::TopLeft | Self::TopCenter | Self::TopRight => "top",
      _ => "bottom",
    }
  }
}

/// Sonner toaster options. The defaults match the login toast.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ToasterOptions {
  pub position: ToastPosition,
  pub expand: bool,
  pub visible_toasts: usize,
  pub rich_colors: bool,
  pub close_button: bool,
  pub gap: u32,
  pub duration: u32,
  pub offset: u32,
  pub mobile_offset: u32,
}

impl Default for ToasterOptions {
  fn default() -> Self {
    Self {
      position: ToastPosition::BottomRight,
      expand: false,
      visible_toasts: 3,
      rich_colors: true,
      close_button: true,
      gap: 14,
      duration: 4000,
      offset: 32,
      mobile_offset: 16,
    }
  }
}

fn clip(value: &str, max: usize) -> String {
  value.trim().chars().take(max).collect()
}

fn clip_link(link: Option<ToastLink>) -> Option<ToastLink> {
  let link = link?;
  let label = clip(&link.label, 80);
  let href = clip(&link.href, 300);
  if label.is_empty() || href.is_empty() {
    None
  } else {
    Some(ToastLink { label, href })
  }
}

fn sanitize(mut toast: Toast) -> Option<Toast> {
  toast.title = clip(&toast.title, TOAST_MAX_CHARS);
  if toast.title.is_empty() {
    return None;
  }
  toast.description = toast
    .description
    .map(|description| clip(&description, TOAST_MAX_CHARS))
    .filter(|description| !description.is_empty());
  toast.action = clip_link(toast.action);
  toast.cancel = clip_link(toast.cancel);
  Some(toast)
}

fn percent_encode(input: &str) -> String {
  let mut out = String::with_capacity(input.len());
  for byte in input.bytes() {
    match byte {
      b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
        out.push(byte as char);
      }
      _ => {
        const HEX: &[u8; 16] = b"0123456789ABCDEF";
        out.push('%');
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0xf) as usize] as char);
      }
    }
  }
  out
}

fn percent_decode(input: &str) -> String {
  let bytes = input.as_bytes();
  let mut out = Vec::with_capacity(bytes.len());
  let mut index = 0;
  while index < bytes.len() {
    if bytes[index] == b'%' && index + 2 < bytes.len() {
      let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).ok();
      let parsed = hex.and_then(|hex| u8::from_str_radix(hex, 16).ok());
      if let Some(parsed) = parsed {
        out.push(parsed);
        index += 3;
        continue;
      }
    }
    out.push(bytes[index]);
    index += 1;
  }
  String::from_utf8_lossy(&out).into_owned()
}

fn parse_queue_body(value: &str) -> Option<Vec<Toast>> {
  if let Some(message) = value.strip_prefix("ok:") {
    return Some(sanitize(Toast::success(message)).into_iter().collect());
  }
  if let Some(message) = value.strip_prefix("err:") {
    return Some(sanitize(Toast::error(message)).into_iter().collect());
  }
  if value.starts_with('[') {
    return serde_json::from_str(value).ok();
  }
  None
}

/// Topcoat percent-encodes cookie values on the way out and decodes them on
/// the way in, so the jar usually holds raw JSON. A value that is still
/// percent-encoded (or a leftover `ok:` / `err:` prefix) is accepted too.
fn decode_queue(value: &str) -> Vec<Toast> {
  let value = value.trim();
  if let Some(toasts) = parse_queue_body(value) {
    return toasts;
  }
  parse_queue_body(&percent_decode(value)).unwrap_or_default()
}

fn enqueue(mut queue: Vec<Toast>, toast: Toast) -> Vec<Toast> {
  queue.push(toast);
  if queue.len() > MAX_TOASTS {
    let overflow = queue.len() - MAX_TOASTS;
    queue.drain(0..overflow);
  }
  while queue.len() > 1 {
    let json = serde_json::to_string(&queue).unwrap_or_default();
    if percent_encode(&json).len() <= COOKIE_BUDGET {
      break;
    }
    queue.remove(0);
  }
  queue
}

fn is_toast_promise(cx: &Cx) -> bool {
  headers(cx)
    .get("x-toast-promise")
    .and_then(|value| value.to_str().ok())
    .is_some_and(|value| value.trim() == "1")
}

fn toast_header_value(toast: &Toast) -> Option<HeaderValue> {
  let json = serde_json::to_string(toast).ok()?;
  HeaderValue::from_str(&percent_encode(&json)).ok()
}

fn location_header(uri: &str) -> HeaderValue {
  if let Ok(value) = HeaderValue::from_str(uri) {
    return value;
  }
  let mut encoded = String::new();
  for ch in uri.chars() {
    if ch.is_ascii() && !ch.is_ascii_control() {
      encoded.push(ch);
    } else {
      let mut buf = [0; 4];
      for byte in ch.encode_utf8(&mut buf).bytes() {
        encoded.push_str(&format!("%{byte:02X}"));
      }
    }
  }
  HeaderValue::from_str(&encoded)
    .unwrap_or_else(|_| HeaderValue::from_static("/"))
}

fn current_queue(cx: &Cx) -> Vec<Toast> {
  cookies(cx)
    .get(TOAST_COOKIE)
    .map(|cookie| decode_queue(cookie.value()))
    .unwrap_or_default()
}

fn write_queue(cx: &Cx, queue: &[Toast]) {
  let json = serde_json::to_string(queue).unwrap_or_else(|_| "[]".to_owned());
  cookies(cx).add(
    Cookie::build((TOAST_COOKIE, json))
      .path("/")
      .max_age(Duration::seconds(60))
      .http_only(true)
      .same_site(SameSite::Lax)
      .build(),
  );
}

/// Toasts from a promise submit, held until [`toast_redirect`] knows whether
/// the browser will load another pathname. Keyed by request so concurrent
/// submits do not share a queue.
fn promise_toasts() -> &'static Mutex<HashMap<CxId, Vec<Toast>>> {
  static PENDING: OnceLock<Mutex<HashMap<CxId, Vec<Toast>>>> = OnceLock::new();
  PENDING.get_or_init(|| Mutex::new(HashMap::new()))
}

fn remember_promise_toast(cx: &Cx, toast: Toast) {
  let mut pending = promise_toasts()
    .lock()
    .unwrap_or_else(PoisonError::into_inner);
  pending.entry(cx.id()).or_default().push(toast);
}

fn take_promise_toasts(cx: &Cx) -> Vec<Toast> {
  let mut pending = promise_toasts()
    .lock()
    .unwrap_or_else(PoisonError::into_inner);
  pending.remove(&cx.id()).unwrap_or_default()
}

fn persist_toasts(cx: &Cx, pending: Vec<Toast>, shown: bool) {
  if pending.is_empty() {
    return;
  }
  let mut queue = current_queue(cx);
  for mut toast in pending {
    toast.shown = shown;
    queue = enqueue(queue, toast);
  }
  write_queue(cx, &queue);
}

/// Pathname of `location` on this request's origin.
///
/// Matches `samePath` in `assets/toast.js`: the query string and fragment are
/// ignored, and a location on another origin is not the same path. `None`
/// means the location is not on `origin`.
fn resolved_pathname(
  request_path: &str,
  origin: Option<&str>,
  location: &str,
) -> Option<String> {
  let location = location.trim();
  if location.is_empty() {
    return None;
  }
  if location.starts_with('#') || location.starts_with('?') {
    return Some(request_path.to_owned());
  }
  if let Some(rest) = location.strip_prefix("//") {
    let scheme = origin.and_then(|origin| origin.split("://").next())?;
    return pathname_if_same_origin(origin, &format!("{scheme}://{rest}"));
  }
  if split_scheme(location).is_some() {
    return pathname_if_same_origin(origin, location);
  }
  if let Some(path) = location.strip_prefix('/') {
    let path = path.split(['?', '#']).next().unwrap_or("");
    return Some(format!("/{path}"));
  }
  let relative = location.split(['?', '#']).next().unwrap_or("");
  Some(resolve_relative(request_path, relative))
}

fn pathname_if_same_origin(
  origin: Option<&str>,
  location: &str,
) -> Option<String> {
  let origin = origin?;
  let target: http::Uri = location.parse().ok()?;
  let scheme = target.scheme_str()?;
  let authority = target.authority()?.as_str();
  if origin_key(scheme, authority) != origin_key_of(origin)? {
    return None;
  }
  let path = target.path();
  if path.is_empty() {
    Some("/".to_owned())
  } else {
    Some(path.to_owned())
  }
}

fn origin_key(scheme: &str, authority: &str) -> String {
  let scheme = scheme.to_ascii_lowercase();
  let authority = authority.to_ascii_lowercase();
  let authority = match scheme.as_str() {
    "http" => authority.strip_suffix(":80").unwrap_or(&authority),
    "https" => authority.strip_suffix(":443").unwrap_or(&authority),
    _ => &authority,
  };
  format!("{scheme}://{authority}")
}

fn origin_key_of(origin: &str) -> Option<String> {
  let uri: http::Uri = origin.parse().ok()?;
  let scheme = uri.scheme_str()?;
  let authority = uri.authority()?.as_str();
  Some(origin_key(scheme, authority))
}

fn split_scheme(location: &str) -> Option<(&str, &str)> {
  let (scheme, rest) = location.split_once(':')?;
  let mut chars = scheme.chars();
  let first = chars.next()?;
  if !first.is_ascii_alphabetic() {
    return None;
  }
  if !chars
    .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '+' | '-' | '.'))
  {
    return None;
  }
  Some((scheme, rest))
}

fn resolve_relative(request_path: &str, relative: &str) -> String {
  let mut segments: Vec<&str> = request_path
    .split('/')
    .filter(|part| !part.is_empty())
    .collect();
  if !request_path.ends_with('/') {
    segments.pop();
  }
  for part in relative.split('/') {
    match part {
      "" | "." => {}
      ".." => {
        segments.pop();
      }
      other => segments.push(other),
    }
  }
  if segments.is_empty() {
    "/".to_owned()
  } else {
    format!("/{}", segments.join("/"))
  }
}

fn request_origin(cx: &Cx) -> Option<String> {
  let host = headers(cx)
    .get(http::header::HOST)
    .and_then(|value| value.to_str().ok())
    .map(str::trim)
    .filter(|host| !host.is_empty())?;
  Some(format!("{}://{host}", request_scheme(cx)))
}

fn request_scheme(cx: &Cx) -> String {
  if let Some(scheme) = uri(cx).scheme_str() {
    return scheme.to_ascii_lowercase();
  }
  let forwarded = headers(cx)
    .get("x-forwarded-proto")
    .and_then(|value| value.to_str().ok())
    .and_then(|value| value.split(',').next())
    .map(str::trim)
    .unwrap_or("");
  if forwarded.eq_ignore_ascii_case("https") {
    "https".to_owned()
  } else {
    "http".to_owned()
  }
}

/// `true` when `location` stays on this request's pathname, query ignored.
fn same_path(cx: &Cx, location: &str) -> bool {
  let request_path = uri(cx).path();
  resolved_pathname(request_path, request_origin(cx).as_deref(), location)
    .as_deref()
    == Some(request_path)
}

/// Queue a toast for the next full-page render.
///
/// A request that sent `X-Toast-Promise: 1` also gets the toast on the
/// `X-Toast` response header, for the in-page card. The cookie is written
/// later, in [`toast_redirect`], and only when that card will be thrown away
/// by a navigation to another pathname.
pub fn set_toast(cx: &Cx, toast: Toast) {
  let Some(toast) = sanitize(toast) else {
    return;
  };
  if is_toast_promise(cx) {
    if let Some(value) = toast_header_value(&toast) {
      response_headers(cx).append(HeaderName::from_static("x-toast"), value);
    }
    remember_promise_toast(cx, toast);
    return;
  }
  write_queue(cx, &enqueue(current_queue(cx), toast));
}

/// Read and clear the toast cookie. Call this from the root layout only.
pub fn take_toasts(cx: &Cx) -> Vec<Toast> {
  let jar = cookies(cx);
  let Some(cookie) = jar.get(TOAST_COOKIE) else {
    return Vec::new();
  };
  let value = cookie.value().to_owned();
  jar.remove(Cookie::build((TOAST_COOKIE, "")).path("/").build());
  decode_queue(&value)
}

/// Redirect after a toast.
///
/// A normal submit is a 303 and keeps the toast cookie [`set_toast`] already
/// wrote. A promise submit (`X-Toast-Promise: 1`) is a 200 that still carries
/// `Location`, because `fetch` with `redirect: "manual"` hides the headers of
/// a real 3xx response. The page script follows `Location` itself. The promise
/// cookie is written here, only when `uri` leaves this pathname: the in-page
/// card covers a same-path response, and a cookie would show that toast again
/// on refresh.
pub fn toast_redirect(cx: &Cx, uri: impl AsRef<str>) -> Result<Response> {
  let uri = uri.as_ref();
  let pending = take_promise_toasts(cx);
  if is_toast_promise(cx) {
    // The in-page card survives a same-pathname response, including a query
    // change such as `/login` → `/login?mode=signin`. A different pathname
    // throws that card away, so the destination has to render the flash.
    if !same_path(cx, uri) {
      // The card was already on screen. The next page keeps it, without
      // playing the enter animation a second time.
      persist_toasts(cx, pending, true);
    }
    return (StatusCode::OK, [(LOCATION, location_header(uri))], "")
      .into_response(cx);
  }
  if !pending.is_empty() {
    persist_toasts(cx, pending, false);
  }
  see_other(uri).into_response(cx)
}

struct ToastMarkup {
  kind: &'static str,
  title: String,
  description: Option<String>,
  duration: String,
  dismissible: &'static str,
  show_close: bool,
  action: Option<ToastLink>,
  cancel: Option<ToastLink>,
  style: String,
  front: &'static str,
  hidden: &'static str,
  expanded: &'static str,
  mounted: &'static str,
}

fn present(toasts: Vec<Toast>, options: &ToasterOptions) -> Vec<ToastMarkup> {
  let visible = options.visible_toasts.max(1);
  let ordered: Vec<Toast> = toasts.into_iter().rev().collect();
  let len = ordered.len();
  let dir: i32 = if options.position.y() == "top" { 1 } else { -1 };
  ordered
    .into_iter()
    .enumerate()
    .map(|(index, toast)| {
      let hidden = index >= visible;
      let scale = if options.expand || index == 0 || hidden {
        "1".to_owned()
      } else {
        format!("{:.3}", (1.0 - index as f64 * 0.05).max(0.85))
      };
      let offset = if hidden || options.expand {
        0
      } else {
        dir * index as i32 * options.gap as i32
      };
      let z = len.saturating_sub(index).max(1);
      ToastMarkup {
        kind: toast.kind.as_str(),
        title: toast.title,
        description: toast.description,
        duration: toast.duration.map(|ms| ms.to_string()).unwrap_or_default(),
        dismissible: if toast.dismissible { "true" } else { "false" },
        show_close: options.close_button && toast.dismissible,
        action: toast.action,
        cancel: toast.cancel,
        style: format!("--offset:{offset}px;--scale:{scale};--z:{z};"),
        front: if index == 0 { "true" } else { "false" },
        hidden: if hidden { "true" } else { "false" },
        expanded: if options.expand { "true" } else { "false" },
        mounted: if toast.shown { "true" } else { "false" },
      }
    })
    .collect()
}

#[component]
async fn toast_icon(kind: &'static str) -> Result<impl View> {
  Ok(view! {
      if kind == "success" {
          icon(
              data: iconify_icon!("lucide:circle-check"),
              attrs: attributes! { class="size-4" },
          )
      } else if kind == "info" {
          icon(
              data: iconify_icon!("lucide:info"),
              attrs: attributes! { class="size-4" },
          )
      } else if kind == "warning" {
          icon(
              data: iconify_icon!("lucide:triangle-alert"),
              attrs: attributes! { class="size-4" },
          )
      } else if kind == "error" {
          icon(
              data: iconify_icon!("lucide:circle-alert"),
              attrs: attributes! { class="size-4" },
          )
      } else if kind == "loading" {
          icon(
              data: iconify_icon!("lucide:loader-circle"),
              attrs: attributes! { class="size-4" },
          )
      }
  })
}

/// Renders the toast stack and the script that drives it, even when `toasts` is empty.
#[component]
pub async fn toaster(
  toasts: Vec<Toast>,
  #[default] options: ToasterOptions,
) -> Result<impl View> {
  let x = options.position.x();
  let y = options.position.y();
  let expand = if options.expand { "true" } else { "false" };
  let rich = if options.rich_colors { "true" } else { "false" };
  let close = if options.close_button {
    "true"
  } else {
    "false"
  };
  let visible = options.visible_toasts.max(1).to_string();
  let gap = options.gap.to_string();
  let duration = options.duration.to_string();
  let shell_style = format!(
    "--toaster-offset: {}px; --toaster-mobile-offset: {}px; --gap: {}px;",
    options.offset, options.mobile_offset, options.gap,
  );
  let markup = present(toasts, &options);

  Ok(view! {
      <section
          data-toaster=""
          data-x=(x)
          data-y=(y)
          data-expand=(expand)
          data-visible-toasts=(visible.as_str())
          data-rich-colors=(rich)
          data-close-button=(close)
          data-gap=(gap.as_str())
          data-duration=(duration.as_str())
          role="region"
          aria-label="Notificações"
          style=(shell_style.as_str())
      >
          <ol data-toast-list="">
              for toast in markup {
                  <li
                      data-toast-item=""
                      data-front=(toast.front)
                      data-hidden=(toast.hidden)
                      data-expanded=(toast.expanded)
                      style=(toast.style.as_str())
                  >
                      <div
                          role="status"
                          data-toast=""
                          data-type=(toast.kind)
                          data-duration=(toast.duration.as_str())
                          data-dismissible=(toast.dismissible)
                          data-mounted=(toast.mounted)
                      >
                          <button
                              type="button"
                              data-toast-close=""
                              aria-label="Fechar"
                              hidden=(!toast.show_close)
                          >
                              icon(
                                  data: iconify_icon!("lucide:x"),
                                  attrs: attributes! { class="size-3" },
                              )
                          </button>
                          <span data-toast-icon="">
                              toast_icon(kind: toast.kind)
                          </span>
                          <div data-toast-body="">
                              <p data-toast-title="">(toast.title.as_str())</p>
                              if let Some(ref description) = toast.description {
                                  <p data-toast-description="">(description.as_str())</p>
                              } else {
                                  <p data-toast-description="" hidden=""></p>
                              }
                          </div>
                          if let Some(ref action) = toast.action {
                              <a data-toast-action="" href=(action.href.as_str())>
                                  (action.label.as_str())
                              </a>
                          } else {
                              <a data-toast-action="" hidden=""></a>
                          }
                          if let Some(ref cancel) = toast.cancel {
                              <a data-toast-cancel="" href=(cancel.href.as_str())>
                                  (cancel.label.as_str())
                              </a>
                          } else {
                              <a data-toast-cancel="" hidden=""></a>
                          }
                      </div>
                  </li>
              }
          </ol>
          <template data-toast-template="">
              <li data-toast-item="">
                  <div role="status" data-toast="" data-type="default" data-dismissible="true">
                      <button type="button" data-toast-close="" aria-label="Fechar">
                          icon(
                              data: iconify_icon!("lucide:x"),
                              attrs: attributes! { class="size-3" },
                          )
                      </button>
                      <span data-toast-icon=""></span>
                      <div data-toast-body="">
                          <p data-toast-title=""></p>
                          <p data-toast-description="" hidden=""></p>
                      </div>
                      <a data-toast-action="" hidden=""></a>
                      <a data-toast-cancel="" hidden=""></a>
                  </div>
              </li>
          </template>
          <div data-toast-icon-bank="" hidden="" aria-hidden="true">
              <span data-icon="success">toast_icon(kind: "success")</span>
              <span data-icon="info">toast_icon(kind: "info")</span>
              <span data-icon="warning">toast_icon(kind: "warning")</span>
              <span data-icon="error">toast_icon(kind: "error")</span>
              <span data-icon="loading">toast_icon(kind: "loading")</span>
          </div>
          <script src=(TOAST_SCRIPT)></script>
      </section>
  })
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn legacy_prefixes_still_decode() {
    let ok = decode_queue("ok:Conta criada.");
    assert_eq!(ok.len(), 1);
    assert_eq!(ok[0].kind, ToastType::Success);
    assert_eq!(ok[0].title, "Conta criada.");

    let err = decode_queue("err:Senha incorreta");
    assert_eq!(err[0].kind, ToastType::Error);
    assert_eq!(err[0].title, "Senha incorreta");
    assert!(decode_queue("err:   ").is_empty());
  }

  #[test]
  fn json_queue_round_trips_through_the_cookie_encoding() {
    let toast = Toast::success("Olá")
      .description("Tudo bem")
      .action("Ver", "/conta");
    let encoded = percent_encode(&serde_json::to_string(&vec![toast]).unwrap());
    let decoded = decode_queue(&encoded);
    assert_eq!(decoded[0].title, "Olá");
    assert_eq!(decoded[0].description.as_deref(), Some("Tudo bem"));
    assert_eq!(
      decoded[0].action.as_ref().map(|link| link.href.as_str()),
      Some("/conta")
    );
    assert_eq!(decoded[0].kind, ToastType::Success);
    assert!(!decoded[0].shown);

    let mut seen = Toast::success("Já estava");
    seen.shown = true;
    let seen = decode_queue(&serde_json::to_string(&vec![seen]).unwrap());
    assert!(seen[0].shown);
    let markup = present(seen, &ToasterOptions::default());
    assert_eq!(markup[0].mounted, "true");
  }

  #[test]
  fn a_query_string_stays_on_the_same_pathname() {
    assert!(
      resolved_pathname("/login", None, "/login?mode=signin").as_deref()
        == Some("/login")
    );
    assert!(
      resolved_pathname("/login", None, "/login?mode=signup").as_deref()
        == Some("/login")
    );
    assert!(
      resolved_pathname("/login", None, "?mode=signin").as_deref()
        == Some("/login")
    );
    assert!(
      resolved_pathname("/login", None, "/login#erro").as_deref()
        == Some("/login")
    );
  }

  #[test]
  fn another_pathname_is_a_different_path() {
    let origin = "http://127.0.0.1:4200";
    assert_eq!(
      resolved_pathname("/login", None, "/dashboard").as_deref(),
      Some("/dashboard")
    );
    assert_eq!(
      resolved_pathname("/login", None, "/login/").as_deref(),
      Some("/login/")
    );
    assert!(
      resolved_pathname(
        "/login",
        Some(origin),
        "https://accounts.google.com/o/oauth2/v2/auth?x=1"
      )
      .is_none()
    );
    assert_eq!(
      resolved_pathname(
        "/login",
        Some(origin),
        "http://127.0.0.1:4200/login?mode=signin"
      )
      .as_deref(),
      Some("/login")
    );
    assert_eq!(
      resolved_pathname(
        "/login",
        Some(origin),
        "http://127.0.0.1:4200/dashboard"
      )
      .as_deref(),
      Some("/dashboard")
    );
    assert!(
      resolved_pathname("/login", Some(origin), "https://127.0.0.1:4200/login")
        .is_none()
    );
  }

  #[test]
  fn queue_keeps_the_newest_five() {
    let mut queue = Vec::new();
    for index in 0..7 {
      queue = enqueue(queue, Toast::info(format!("n{index}")));
    }
    assert_eq!(queue.len(), 5);
    assert_eq!(queue[0].title, "n2");
    assert_eq!(queue[4].title, "n6");
  }
}
