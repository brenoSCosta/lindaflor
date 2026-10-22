use crate::components::breadcrumb::{
  breadcrumb, breadcrumb_item, breadcrumb_link, breadcrumb_list,
  breadcrumb_page, breadcrumb_separator,
};
use crate::components::card::{card, card_content, card_header, card_title};
use crate::components::container::{ContainerVariant, container};
use crate::components::table::{
  table, table_body, table_cell, table_head, table_header, table_row,
};
use topcoat::{
  Result,
  context::Cx,
  router::{href, page},
  view::{View, attributes, view},
};

#[page]
pub async fn page(cx: &Cx) -> Result<impl View> {
  crate::seo::set_head(
    cx,
    crate::seo::PageHead::new("Guia de Medidas")
      .description("Guia de medidas Linda Flor: tabela de tamanhos PP a GG para biquínis, maiôs e saídas de praia.")
      .canonical("/guia-de-medidas"),
  );
  Ok(view! {
      container(
          variant: ContainerVariant::Narrow,
          breadcrumb(
              breadcrumb_list(
                  breadcrumb_item(breadcrumb_link(attrs: attributes! { href=(href!(crate::app::page)) }, "Início"))
                  breadcrumb_separator()
                  breadcrumb_item(breadcrumb_page("Guia de medidas"))
              )
          )
          <h1 class="text-4xl font-bold tracking-tight">"Guia de medidas"</h1>

          <section class="space-y-4">
              card(
                  card_header(
                      card_title("Como medir")
                  )
                  card_content(
                      <div class="space-y-4 text-sm text-muted-foreground">
                          <div>
                              <p class="font-semibold text-foreground">"Busto"</p>
                              <p>"Meça ao redor da parte mais larga do tórax, mantendo a fita métrica paralela ao chão. Não aperte muito a fita."</p>
                          </div>
                          <div>
                              <p class="font-semibold text-foreground">"Cintura"</p>
                              <p>"Meça na parte mais estreita da cintura, entre as costelas e o quadril. A fita deve estar confortável, sem apertar."</p>
                          </div>
                          <div>
                              <p class="font-semibold text-foreground">"Quadril"</p>
                              <p>"Meça ao redor da parte mais larga das nádegas, mantendo a fita métrica paralela ao chão."</p>
                          </div>
                      </div>
                  )
              )
          </section>

          <section class="space-y-4">
              card(
                  card_header(
                      card_title("Tops")
                  )
                  card_content(
                      table(
                          table_header(
                              table_row(
                                  table_head("Tamanho")
                                  table_head("Busto (cm)")
                              )
                          )
                          table_body(
                              table_row(
                                  table_cell("PP")
                                  table_cell("76 - 82")
                              )
                              table_row(
                                  table_cell("P")
                                  table_cell("82 - 88")
                              )
                              table_row(
                                  table_cell("M")
                                  table_cell("88 - 94")
                              )
                              table_row(
                                  table_cell("G")
                                  table_cell("94 - 100")
                              )
                              table_row(
                                  table_cell("GG")
                                  table_cell("100 - 106")
                              )
                          )
                      )
                  )
              )
          </section>

          <section class="space-y-4">
              card(
                  card_header(
                      card_title("Calcinhas")
                  )
                  card_content(
                      table(
                          table_header(
                              table_row(
                                  table_head("Tamanho")
                                  table_head("Cintura (cm)")
                                  table_head("Quadril (cm)")
                              )
                          )
                          table_body(
                              table_row(
                                  table_cell("PP")
                                  table_cell("62 - 66")
                                  table_cell("88 - 92")
                              )
                              table_row(
                                  table_cell("P")
                                  table_cell("66 - 70")
                                  table_cell("92 - 96")
                              )
                              table_row(
                                  table_cell("M")
                                  table_cell("70 - 74")
                                  table_cell("96 - 100")
                              )
                              table_row(
                                  table_cell("G")
                                  table_cell("74 - 78")
                                  table_cell("100 - 104")
                              )
                              table_row(
                                  table_cell("GG")
                                  table_cell("78 - 82")
                                  table_cell("104 - 108")
                              )
                          )
                      )
                  )
              )
          </section>

          <section class="space-y-4">
              card(
                  card_header(
                      card_title("Maiôs")
                  )
                  card_content(
                      table(
                          table_header(
                              table_row(
                                  table_head("Tamanho")
                                  table_head("Busto (cm)")
                                  table_head("Cintura (cm)")
                                  table_head("Quadril (cm)")
                              )
                          )
                          table_body(
                              table_row(
                                  table_cell("PP")
                                  table_cell("76 - 82")
                                  table_cell("62 - 66")
                                  table_cell("88 - 92")
                              )
                              table_row(
                                  table_cell("P")
                                  table_cell("82 - 88")
                                  table_cell("66 - 70")
                                  table_cell("92 - 96")
                              )
                              table_row(
                                  table_cell("M")
                                  table_cell("88 - 94")
                                  table_cell("70 - 74")
                                  table_cell("96 - 100")
                              )
                              table_row(
                                  table_cell("G")
                                  table_cell("94 - 100")
                                  table_cell("74 - 78")
                                  table_cell("100 - 104")
                              )
                              table_row(
                                  table_cell("GG")
                                  table_cell("100 - 106")
                                  table_cell("78 - 82")
                                  table_cell("104 - 108")
                              )
                          )
                      )
                  )
              )
          </section>

          <section>
              card(
                  card_content(
                      <p class="text-sm text-muted-foreground">
                          <strong>"Dica:"</strong>" Se você está entre dois tamanhos, recomendamos escolher o tamanho maior para mais conforto."
                      </p>
                  )
              )
          </section>
      )
  })
}
