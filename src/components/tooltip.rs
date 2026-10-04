use topcoat::{
  Result,
  view::{Attributes, Child, StaticClass, View, class, component, view},
};

/// A short hint shown when its trigger is hovered or focused.
///
/// Pass the trigger and `tooltip_content` as children. The bubble does not
/// flip itself to stay in view: pick `side` and `align` for the trigger's
/// screen position (e.g. a toolbar pinned to the top edge needs
/// `TooltipSide::Bottom`), and keep the hint short.
///
/// Give the trigger its own text or accessible label. The tooltip must not be the only
/// way to learn what the trigger does. To associate the hint with the trigger, give
/// `tooltip_content` an `id` and reference it with the trigger's `aria-describedby`.
///
/// ```ignore
/// view! {
///     tooltip(
///         button(
///             size: ButtonSize::Icon,
///             variant: ButtonVariant::Outline,
///             icon(data: iconify_icon!("lucide:copy"), label: "Copy link")
///         )
///         tooltip_content("Copy link")
///     )
/// }
/// ```
#[component]
pub async fn tooltip(
  #[default] mut attrs: Attributes,
  #[default] child: Child<'_>,
) -> Result<impl View> {
  Ok(view! {
      <span
          class=(class!("group relative inline-flex", attrs.remove("class")))
          (attrs)
      >
          (child)
      </span>
  })
}

/// Which side of its trigger a [`tooltip_content`] bubble appears on.
///
/// [`Top`](TooltipSide::Top) is the default and matches the previous static
/// positioning. Triggers pinned to a viewport edge need the opposite side
/// (e.g. [`Bottom`](TooltipSide::Bottom) for a sticky top toolbar), otherwise
/// the bubble renders off-screen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TooltipSide {
  /// Above the trigger.
  #[default]
  Top,
  /// Below the trigger.
  Bottom,
  /// To the left of the trigger.
  Left,
  /// To the right of the trigger.
  Right,
}

/// How a [`tooltip_content`] bubble aligns on its side's cross axis.
///
/// [`Center`](TooltipAlign::Center) is the default. Near a viewport edge,
/// [`Start`](TooltipAlign::Start) or [`End`](TooltipAlign::End) keeps a wide
/// bubble from overflowing (e.g. `Bottom` + `Start` for a trigger in a
/// left-aligned toolbar).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TooltipAlign {
  /// Aligned to the trigger's leading edge.
  Start,
  /// Centered on the trigger.
  #[default]
  Center,
  /// Aligned to the trigger's trailing edge.
  End,
}

/// Classes for a tooltip bubble without its position. Opacity and visibility
/// transitions let it fade in and out. The bubble ignores pointer events.
const BUBBLE: StaticClass = class!(
  "pointer-events-none invisible absolute z-50 rounded-md bg-foreground px-2.5 py-1 text-xs font-medium text-background \
     opacity-0 shadow-sm whitespace-nowrap \
     [transition:opacity_150ms_ease-out,visibility_150ms_allow-discrete] \
     group-hover:visible group-hover:opacity-100 \
     group-focus-within:visible group-focus-within:opacity-100",
);

impl TooltipSide {
  /// Position classes for a `side` + `align` pair, kept as one match so
  /// invalid combinations are unrepresentable.
  fn position(self, align: TooltipAlign) -> StaticClass {
    match (self, align) {
      (Self::Top, TooltipAlign::Start) => class!("bottom-full left-0 mb-2"),
      (Self::Top, TooltipAlign::Center) => {
        class!("bottom-full left-1/2 mb-2 -translate-x-1/2")
      }
      (Self::Top, TooltipAlign::End) => class!("right-0 bottom-full mb-2"),
      (Self::Bottom, TooltipAlign::Start) => class!("top-full left-0 mt-2"),
      (Self::Bottom, TooltipAlign::Center) => {
        class!("top-full left-1/2 mt-2 -translate-x-1/2")
      }
      (Self::Bottom, TooltipAlign::End) => class!("top-full right-0 mt-2"),
      (Self::Left, TooltipAlign::Start) => class!("top-0 right-full mr-2"),
      (Self::Left, TooltipAlign::Center) => {
        class!("top-1/2 right-full mr-2 -translate-y-1/2")
      }
      (Self::Left, TooltipAlign::End) => class!("right-full bottom-0 mr-2"),
      (Self::Right, TooltipAlign::Start) => class!("top-0 left-full ml-2"),
      (Self::Right, TooltipAlign::Center) => {
        class!("top-1/2 left-full ml-2 -translate-y-1/2")
      }
      (Self::Right, TooltipAlign::End) => class!("bottom-0 left-full ml-2"),
    }
  }
}

/// The hint a [`tooltip`] shows, in a bubble beside its trigger.
#[component]
pub async fn tooltip_content(
  #[default] side: TooltipSide,
  #[default] align: TooltipAlign,
  #[default] mut attrs: Attributes,
  #[default] child: Child<'_>,
) -> Result<impl View> {
  Ok(view! {
      <span
          role="tooltip"
          class=(class!(BUBBLE, side.position(align), attrs.remove("class")))
          (attrs)
      >
          (child)
      </span>
  })
}
