use topcoat::{
  Result,
  asset::{Asset, asset},
  icon::{icon, iconify::iconify_icon},
  view::{
    Attributes, Child, StaticClass, View, attributes, class, component, view,
  },
};

/// Shared keyboard helpers (`window.__tcHotkey`): platform detection and
/// `Escape` / macOS `Cmd+.` dismissal matching used by the menu script below.
const HOTKEY_SCRIPT: Asset = asset!("assets/hotkey.js");

/// Content-hashed URL for the dropdown-menu dismissal script.
///
/// A native `<details>` element does not close itself when clicking elsewhere,
/// so the primitive ships this scripting. It is guarded so re-rendered menus
/// bind it only once. Open menus dismiss on outside click and `Escape`
/// (`Cmd+.` too on macOS); activating a link or button inside a menu closes
/// that menu.
const DROPDOWN_MENU_SCRIPT: Asset = asset!("assets/dropdown-menu.js");

/// A floating action menu controlled by a trigger.
///
/// Uses a native `<details>` element, so the trigger opens and closes it without
/// JavaScript. Open menus dismiss on outside click and `Escape` (`Cmd+.` too
/// on macOS) via the primitive's own scripting; activating a link or button
/// inside also closes its menu. `attrs` are forwarded to the `<details>`,
/// with extra classes added to its classes.
/// Items use normal Tab navigation. The component does not implement the ARIA menu
/// pattern's arrow-key navigation.
///
/// ```ignore
/// view! {
///     dropdown_menu(
///         dropdown_menu_trigger("Options")
///         dropdown_menu_content(
///             dropdown_menu_item("Rename")
///             dropdown_menu_item("Duplicate")
///             dropdown_menu_separator()
///             dropdown_menu_item(
///                 attrs: attributes! { class="text-destructive" },
///                 "Delete"
///             )
///         )
///     )
/// }
/// ```
#[component]
pub async fn dropdown_menu(
  #[default] mut attrs: Attributes,
  #[default] child: Child<'_>,
) -> Result<impl View> {
  Ok(view! {
      <details
          data-dropdown-menu=""
          class=(class!("group relative inline-block", attrs.remove("class")))
          (attrs)
      >
          (child)
          <script src=(HOTKEY_SCRIPT)></script>
          <script src=(DROPDOWN_MENU_SCRIPT)></script>
      </details>
  })
}

/// Classes that hide the native disclosure marker and show a pointer cursor.
const TRIGGER: StaticClass =
  class!("cursor-pointer list-none [&::-webkit-details-marker]:hidden",);

/// A trigger that opens or closes the dropdown menu.
///
/// Pass its label as children. To style it as a button, pass classes from
/// [`button_variants`](super::button::button_variants) through `attrs`. The attributes
/// go on the `<summary>`. Use `group-open:` classes to style children while the menu is
/// open.
///
/// ```ignore
/// view! {
///     dropdown_menu_trigger(
///         attrs: attributes! {
///             class=(button_variants(ButtonVariant::Outline, ButtonSize::Md))
///         },
///         "Options"
///         icon(
///             data: iconify_icon!("lucide:chevron-down"),
///             attrs: attributes! { class="group-open:rotate-180" }
///         )
///     )
/// }
/// ```
#[component]
pub async fn dropdown_menu_trigger(
  #[default] mut attrs: Attributes,
  #[default] child: Child<'_>,
) -> Result<impl View> {
  Ok(view! {
      <summary class=(class!(TRIGGER, attrs.remove("class"))) (attrs)>
          (child)
      </summary>
  })
}

/// Classes for floating menu panels with their own background, border, and text color.
const PANEL: StaticClass = class!(
  "absolute z-50 min-w-40 rounded-lg border border-border bg-popover p-1 \
     text-popover-foreground shadow-sm",
);

/// Which side of the trigger the menu panel opens on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DropdownMenuSide {
  /// Panel opens above the trigger.
  Top,
  /// Panel opens to the right of the trigger.
  Right,
  /// Panel opens below the trigger.
  #[default]
  Bottom,
  /// Panel opens to the left of the trigger.
  Left,
}

/// How the menu panel lines up with the trigger along the [`DropdownMenuSide`]
/// axis.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DropdownMenuAlign {
  /// Panel's start edge (left for top/bottom sides, top for left/right
  /// sides) meets the trigger's start edge.
  #[default]
  Start,
  /// Panel is centered on the trigger.
  Center,
  /// Panel's end edge (right for top/bottom sides, bottom for left/right
  /// sides) meets the trigger's end edge. Useful for triggers at the right
  /// edge of a table or viewport.
  End,
}

/// The floating panel of a [`dropdown_menu`], holding the menu's items.
///
/// The panel floats above surrounding content: the primitive's scripting pins
/// it with `fixed` positioning measured from the trigger when the menu opens,
/// so it escapes `overflow` ancestors such as table scrollers instead of
/// being clipped inside them, and keeps it glued to the trigger across
/// scrolling and resizing (flipping to the opposite side when there is no
/// room).
///
/// Placement follows `side` (defaults to [`DropdownMenuSide::Bottom`]) with a
/// `side_offset` gap in pixels, and `align` (defaults to
/// [`DropdownMenuAlign::Start`]) with an `align_offset` nudge in pixels along
/// the alignment axis (positive shifts rightward for top/bottom sides and
/// downward for left/right sides).
#[component]
pub async fn dropdown_menu_content(
  #[default] mut attrs: Attributes,
  #[default] side: DropdownMenuSide,
  #[default(4)] side_offset: i32,
  #[default] align: DropdownMenuAlign,
  #[default] align_offset: i32,
  #[default] child: Child<'_>,
) -> Result<impl View> {
  let side_name = match side {
    DropdownMenuSide::Top => "top",
    DropdownMenuSide::Right => "right",
    DropdownMenuSide::Bottom => "bottom",
    DropdownMenuSide::Left => "left",
  };
  let align_name = match align {
    DropdownMenuAlign::Start => "start",
    DropdownMenuAlign::Center => "center",
    DropdownMenuAlign::End => "end",
  };
  let side_offset_name = side_offset.to_string();
  let align_offset_name = align_offset.to_string();
  // No-JavaScript fallback positioning; the scripting overrides these with
  // exact `fixed` coordinates whenever it runs.
  let (side_class, align_class) = match side {
    DropdownMenuSide::Top => (
      "bottom-full mb-1",
      match align {
        DropdownMenuAlign::Start => "left-0",
        DropdownMenuAlign::Center => "left-1/2 -translate-x-1/2",
        DropdownMenuAlign::End => "right-0",
      },
    ),
    DropdownMenuSide::Right => (
      "left-full ml-1",
      match align {
        DropdownMenuAlign::Start => "top-0",
        DropdownMenuAlign::Center => "top-1/2 -translate-y-1/2",
        DropdownMenuAlign::End => "bottom-0",
      },
    ),
    DropdownMenuSide::Bottom => (
      "top-full mt-1",
      match align {
        DropdownMenuAlign::Start => "left-0",
        DropdownMenuAlign::Center => "left-1/2 -translate-x-1/2",
        DropdownMenuAlign::End => "right-0",
      },
    ),
    DropdownMenuSide::Left => (
      "right-full mr-1",
      match align {
        DropdownMenuAlign::Start => "top-0",
        DropdownMenuAlign::Center => "top-1/2 -translate-y-1/2",
        DropdownMenuAlign::End => "bottom-0",
      },
    ),
  };
  Ok(view! {
      <div
          data-dropdown-menu-content=""
          data-side=(side_name)
          data-side-offset=(side_offset_name)
          data-align=(align_name)
          data-align-offset=(align_offset_name)
          class=(class!(PANEL, side_class, align_class, attrs.remove("class"),))
          (attrs)
      >
          (child)
      </div>
  })
}

/// Classes for a menu item and its interaction states.
const ITEM: StaticClass = class!(
  "flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm \
     whitespace-nowrap outline-none hover:bg-foreground/5 focus-visible:bg-foreground/5 \
     active:bg-foreground/10 disabled:pointer-events-none disabled:opacity-50",
);

/// One action in a [`dropdown_menu_content`], rendered as a `<button>`.
#[component]
pub async fn dropdown_menu_item(
  #[default] mut attrs: Attributes,
  #[default] child: Child<'_>,
) -> Result<impl View> {
  Ok(view! {
      <button class=(class!(ITEM, attrs.remove("class"))) (attrs)>(child)</button>
  })
}

/// A nested menu that opens from a row in the parent menu.
///
/// Its trigger toggles a native `<details>` element without JavaScript. Closing the
/// parent hides the submenu but preserves its open state. Resetting that state requires
/// application scripting.
///
/// Use `group-open/sub:` classes to style children while the submenu is open. `attrs`
/// are forwarded to the `<details>`, with extra classes added to its classes.
///
/// ```ignore
/// view! {
///     dropdown_menu_content(
///         dropdown_menu_item("Back")
///         dropdown_menu_sub(
///             dropdown_menu_sub_trigger("Move to")
///             dropdown_menu_sub_content(
///                 dropdown_menu_item("Inbox")
///                 dropdown_menu_item("Archive")
///             )
///         )
///     )
/// }
/// ```
#[component]
pub async fn dropdown_menu_sub(
  #[default] mut attrs: Attributes,
  #[default] child: Child<'_>,
) -> Result<impl View> {
  Ok(view! {
      <details class=(class!("group/sub relative", attrs.remove("class"))) (attrs)>
          (child)
      </details>
  })
}

/// The row that opens or closes a submenu.
///
/// Pass its label as children. A chevron points toward the submenu, and the row stays
/// highlighted while it is open. `attrs` are forwarded to the `<summary>`, with extra
/// classes added to its classes.
#[component]
pub async fn dropdown_menu_sub_trigger(
  #[default] mut attrs: Attributes,
  #[default] child: Child<'_>,
) -> Result<impl View> {
  Ok(view! {
      <summary
          class=(class!(
              ITEM,
              TRIGGER,
              "group-open/sub:bg-foreground/5",
              attrs.remove("class"),
          ))
          (attrs)
      >
          (child)
          icon(
              data: iconify_icon!("lucide:chevron-right"),
              attrs: attributes! { class="ml-auto size-4" }
          )
      </summary>
  })
}

/// The submenu panel, positioned to the right of its trigger row.
#[component]
pub async fn dropdown_menu_sub_content(
  #[default] mut attrs: Attributes,
  #[default] child: Child<'_>,
) -> Result<impl View> {
  Ok(view! {
      <div
          class=(class!(PANEL, "top-0 left-full ml-1", attrs.remove("class")))
          (attrs)
      >
          (child)
      </div>
  })
}

/// A non-interactive heading grouping the items after it.
#[component]
pub async fn dropdown_menu_label(
  #[default] mut attrs: Attributes,
  #[default] child: Child<'_>,
) -> Result<impl View> {
  Ok(view! {
      <p
          class=(class!(
              "px-2 py-1.5 text-xs font-medium text-muted-foreground",
              attrs.remove("class"),
          ))
          (attrs)
      >
          (child)
      </p>
  })
}

/// A hairline rule separating groups of items.
#[component]
pub async fn dropdown_menu_separator(
  #[default] mut attrs: Attributes,
) -> Result<impl View> {
  Ok(view! {
      <hr class=(class!("-mx-1 my-1 border-border", attrs.remove("class"))) (attrs)>
  })
}
