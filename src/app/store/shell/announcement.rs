use topcoat::{
  Result,
  context::Cx,
  view::{View, component, view},
};

use crate::app::store::pricing::storefront_settings;
use crate::app::store::queries::format_price;

#[component]
pub async fn announcement_bar(cx: &Cx) -> Result<impl View> {
  let settings = storefront_settings(cx).await;

  let announcement_text = if let Some(custom) = &settings.announcement_text {
    custom.clone()
  } else {
    // Build default announcement from settings
    let mut parts = Vec::new();

    if settings.pix_discount_percent > 0 {
      parts.push(format!("{}% OFF NO PIX", settings.pix_discount_percent));
    }

    if settings.free_shipping_threshold_cents > 0 {
      parts.push(format!(
        "FRETE GRÁTIS ACIMA DE {}",
        format_price(settings.free_shipping_threshold_cents)
      ));
    } else {
      parts.push("FRETE GRÁTIS".to_string());
    }

    parts.push("ARACAJU, SE".to_string());
    parts.join(" · ")
  };

  Ok(view! {
      <div class="border-b border-border bg-background px-4 py-2 text-center text-[10px] tracking-widest text-muted-foreground uppercase">
          (announcement_text)
      </div>
  })
}
