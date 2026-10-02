use crate::components::breadcrumb::{
  breadcrumb, breadcrumb_item, breadcrumb_link, breadcrumb_list,
  breadcrumb_page, breadcrumb_separator,
};
use crate::components::card::{card, card_content, card_header, card_title};
use crate::components::separator::separator;
use topcoat::{
  Result,
  router::page,
  view::{View, attributes, view},
};

#[page]
pub async fn page() -> Result<impl View> {
  Ok(view! {
      <div class="mx-auto max-w-3xl px-4 py-8 md:px-8">
          breadcrumb(
              breadcrumb_list(
                  breadcrumb_item(breadcrumb_link(attrs: attributes! { href="/" }, "Início"))
                  breadcrumb_separator()
                  breadcrumb_item(breadcrumb_page("Trocas e Devoluções"))
              )
          )
          <h1 class="mt-6 text-4xl font-bold tracking-tight">"Trocas e Devoluções"</h1>
          <p class="mt-4 text-sm leading-relaxed text-muted-foreground">
              "Você pode solicitar troca ou devolução em até 7 dias corridos após o recebimento do pedido, conforme o Código de Defesa do Consumidor."
          </p>
          <p class="mt-2 text-sm leading-relaxed text-muted-foreground">
              "A peça deve estar sem uso, com etiquetas e na embalagem original. Para iniciar o processo, entre em contato pelo WhatsApp (79) 99816-5115 informando o número do pedido."
          </p>
          <p class="mt-2 text-sm leading-relaxed text-muted-foreground">
              "O frete de devolução por arrependimento é por conta do cliente, exceto em casos de defeito ou erro no envio. Após análise, o reembolso será feito via PIX em até 10 dias úteis."
          </p>
          separator(attrs: attributes! { class="my-8" })
          card(
              card_header(card_title("Como Solicitar uma Troca ou Devolução"))
              card_content(
                  <ol class="list-decimal space-y-2 pl-5 text-sm leading-relaxed text-muted-foreground">
                      <li>"Entre em contato pelo WhatsApp (79) 99816-5115"</li>
                      <li>"Informe o número do pedido e o motivo da troca/devolução"</li>
                      <li>"Envie a peça na embalagem original com todas as etiquetas"</li>
                      <li>"Após análise, o reembolso será feito via PIX em até 10 dias úteis"</li>
                  </ol>
              )
          )
          <div class="mt-4">
              card(
                  card_header(card_title("Condições para Troca ou Devolução"))
                  card_content(
                      <ul class="list-disc space-y-2 pl-5 text-sm leading-relaxed text-muted-foreground">
                          <li>"A peça deve estar sem uso"</li>
                          <li>"Todas as etiquetas devem estar presentes"</li>
                          <li>"A embalagem original deve estar intacta"</li>
                          <li>"O prazo de 7 dias deve ser respeitado"</li>
                      </ul>
                  )
              )
          </div>
          <div class="mt-4">
              card(
                  card_header(card_title("Reembolso"))
                  card_content(
                      <p class="text-sm leading-relaxed text-muted-foreground">
                          "O reembolso será feito via PIX em até 10 dias úteis após a análise da peça devolvida. O valor do reembolso inclui o valor do produto, mas não o frete de devolução por arrependimento."
                      </p>
                  )
              )
          </div>
      </div>
  })
}
