use topcoat::{
  Result,
  view::{Attributes, Child, StaticClass, View, class, component, view},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ContainerVariant {
  #[default]
  Default,
  Wide,
  Narrow,
  Centered,
}

impl ContainerVariant {
  /// Named query container. Padding lives on the inner layout so `@sm/page:`
  /// can see this element's width — a container cannot query itself.
  fn shell(self) -> StaticClass {
    match self {
      Self::Default => {
        class!("@container/page mx-auto w-full max-w-7xl min-w-0")
      }
      Self::Wide => class!("@container/page mx-auto w-full max-w-6xl min-w-0"),
      Self::Narrow => {
        class!("@container/page mx-auto w-full max-w-3xl min-w-0")
      }
      Self::Centered => {
        class!(
          "@container/page mx-auto flex min-h-[70vh] w-full max-w-md min-w-0 flex-col justify-center"
        )
      }
    }
  }

  fn layout(self, flush: bool) -> StaticClass {
    match (self, flush) {
      (Self::Centered, _) => {
        class!(
          "flex w-full min-w-0 flex-col items-center gap-6 px-4 py-8 @sm/page:px-6 @lg/page:px-8"
        )
      }
      (_, true) => {
        class!("flex w-full min-w-0 flex-col px-4 @sm/page:px-6 @lg/page:px-8")
      }
      (_, false) => {
        class!(
          "flex w-full min-w-0 flex-col gap-6 px-4 py-8 @sm/page:px-6 @lg/page:px-8"
        )
      }
    }
  }
}

#[component]
pub async fn container(
  #[default] variant: ContainerVariant,
  #[default] flush: bool,
  #[default] mut attrs: Attributes,
  #[default] child: Child<'_>,
) -> Result<impl View> {
  Ok(view! {
      <div class=(variant.shell())>
          <div class=(class!(variant.layout(flush), attrs.remove("class"))) (attrs)>
              (child)
          </div>
      </div>
  })
}
