use topcoat::{
  Result,
  runtime::{Event, Expr, Signal},
  view::{Attributes, StaticClass, View, class, component, view},
};

/// Classes for the input's dimensions, border, and interaction states.
const INPUT: StaticClass = class!(
  "h-9 w-full min-w-0 rounded-lg border border-border bg-transparent px-3 \
     text-sm transition-colors outline-none \
     placeholder:text-muted-foreground \
     file:mr-3 file:h-full file:border-0 file:bg-transparent file:text-sm file:font-medium \
     focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 \
     aria-invalid:border-destructive aria-invalid:focus-visible:ring-destructive \
     focus-visible:ring-offset-background disabled:pointer-events-none disabled:opacity-50",
);

/// A styled input.
///
/// Pass input attributes and event handlers through `attrs`. Extra classes are added to
/// the input's classes. It fills its container by default. Set `aria-invalid="true"` to
/// show the error border and focus ring.
///
/// Bind `value` to sync the input with a string signal, `touched` to mark the field
/// visited on input or blur, and `error` to drive `aria-invalid` from a live error
/// message. Omit them for an uncontrolled input.
///
/// ```ignore
/// view! {
///     input(attrs: attributes! { type="email" placeholder="you@example.com" })
/// }
/// ```
#[component]
pub async fn input(
  /// Two-way binding for the input's value.
  #[into]
  #[default]
  value: Option<Signal<String>>,
  /// Marked `true` on input or blur. Combine with `error` for touched-only messages.
  #[into]
  #[default]
  touched: Option<Signal<bool>>,
  /// Live error message. A non-empty message sets `aria-invalid="true"`.
  #[into]
  #[default(String::new().into())]
  error: Expr<String>,
  #[default] mut attrs: Attributes,
) -> Result<impl View> {
  let touched_on_blur = touched.clone();
  Ok(view! {
      <input
          class=(class!(INPUT, attrs.remove("class")))
          (attrs)
          if let Some(v) = value {
              :value=$(v.get())
              if let Some(t) = touched {
                  @input=$(|e: Event| {
                      v.set(e.target.value);
                      t.set(true);
                  })
              } else {
                  @input=$(|e: Event| v.set(e.target.value))
              }
          }
          if let Some(t) = touched_on_blur {
              @blur=$(|_e: Event| t.set(true))
          }
          :aria-invalid=$(if error.is_empty() { "false" } else { "true" })
      >
  })
}
