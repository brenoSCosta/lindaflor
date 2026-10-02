use serde::{Deserialize, Serialize};
use topcoat::{
  context::Cx,
  cookie::{Cookie, Cookies, cookies},
};

pub const CART_COOKIE: &str = "lindaflor_cart";

#[derive(Serialize, Deserialize, Clone)]
pub struct CartItem {
  pub variant_id: String,
  pub product_id: String,
  pub product_slug: String,
  pub product_name: String,
  pub variant_label: String,
  pub image_url: Option<String>,
  pub unit_price_cents: i32,
  pub quantity: i32,
  pub max_quantity: i32,
}

pub fn read_cart(cx: &Cx) -> Vec<CartItem> {
  let jar = cookies(cx);
  jar
    .get(CART_COOKIE)
    .and_then(|c| serde_json::from_str(c.value()).ok())
    .unwrap_or_default()
}

pub fn write_cart(cx: &Cx, items: &[CartItem]) {
  let jar = cookies(cx);
  if items.is_empty() {
    jar.remove(Cookie::build((CART_COOKIE, "")).path("/").build());
  } else {
    let json = serde_json::to_string(items).unwrap_or_default();
    jar.add(Cookie::build((CART_COOKIE, json)).path("/").build());
  }
}

pub fn add_to_cart(cx: &Cx, item: CartItem) {
  let mut items = read_cart(cx);
  if let Some(existing) =
    items.iter_mut().find(|i| i.variant_id == item.variant_id)
  {
    existing.quantity =
      (existing.quantity + item.quantity).min(item.max_quantity);
  } else {
    items.push(item);
  }
  write_cart(cx, &items);
}

pub fn update_quantity(cx: &Cx, variant_id: &str, quantity: i32) {
  let mut items = read_cart(cx);
  if quantity <= 0 {
    items.retain(|i| i.variant_id != variant_id);
  } else if let Some(item) =
    items.iter_mut().find(|i| i.variant_id == variant_id)
  {
    item.quantity = quantity.min(item.max_quantity);
  }
  write_cart(cx, &items);
}

pub fn remove_item(cx: &Cx, variant_id: &str) {
  let mut items = read_cart(cx);
  items.retain(|i| i.variant_id != variant_id);
  write_cart(cx, &items);
}

pub fn clear_cart(cx: &Cx) {
  write_cart(cx, &[]);
}

pub fn cart_subtotal_cents(items: &[CartItem]) -> i32 {
  items.iter().map(|i| i.unit_price_cents * i.quantity).sum()
}

pub fn cart_item_count(items: &[CartItem]) -> i32 {
  items.iter().map(|i| i.quantity).sum()
}
