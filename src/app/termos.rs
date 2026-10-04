use crate::components::breadcrumb::{
  breadcrumb, breadcrumb_item, breadcrumb_link, breadcrumb_list,
  breadcrumb_page, breadcrumb_separator,
};
use crate::components::card::{card, card_content, card_header, card_title};
use crate::components::container::{ContainerVariant, container};
use crate::components::separator::separator;
use topcoat::{
  Result,
  router::{href, page},
  view::{View, attributes, view},
};

#[page]
pub async fn page() -> Result<impl View> {
  Ok(view! {
      container(
          variant: ContainerVariant::Narrow,
          breadcrumb(
              breadcrumb_list(
                  breadcrumb_item(breadcrumb_link(attrs: attributes! { href=(href!(crate::app::page)) }, "Início"))
                  breadcrumb_separator()
                  breadcrumb_item(breadcrumb_page("Termos de Uso"))
              )
          )
          <h1 class="mt-6 text-4xl font-bold tracking-tight">"Termos de Uso"</h1>
          <p class="mt-4 text-sm leading-relaxed text-muted-foreground">
              "Ao utilizar o site da Linda Flor Moda Praia, você concorda com estes termos. Os preços, disponibilidade e descrições dos produtos podem ser alterados sem aviso prévio."
          </p>
          <p class="mt-2 text-sm leading-relaxed text-muted-foreground">
              "Pedidos estão sujeitos à confirmação de pagamento e disponibilidade em estoque. Reservamo-nos o direito de cancelar pedidos em caso de inconsistências ou suspeita de fraude."
          </p>
          <p class="mt-2 text-sm leading-relaxed text-muted-foreground">
              "Imagens são ilustrativas. Pequenas variações de cor podem ocorrer devido à calibração de tela e processo de fabricação têxtil."
          </p>
          separator(attrs: attributes! { class="my-8" })
          card(
              card_header(card_title("1. Aceitação dos Termos"))
              card_content(
                  <p class="text-sm leading-relaxed text-muted-foreground">
                      "Ao acessar e utilizar este site, você declara que leu, compreendeu e aceita todos os termos e condições aqui descritos. Se você não concorda com estes termos, por favor, não utilize este site."
                  </p>
              )
          )
    
              card(
                  card_header(card_title("2. Uso do Site"))
                  card_content(
                      <p class="text-sm leading-relaxed text-muted-foreground">
                          "Este site é destinado à venda de produtos de moda praia. Você concorda em utilizar este site apenas para fins lícitos e de acordo com todos os termos e condições aqui descritos."
                      </p>
                  )
              )
    
    
              card(
                  card_header(card_title("3. Produtos e Preços"))
                  card_content(
                      <p class="text-sm leading-relaxed text-muted-foreground">
                          "Empenhamo-nos em fornecer descrições e imagens precisas dos produtos. No entanto, não garantimos que as descrições, imagens ou outros conteúdos do site sejam completos, precisos, confiáveis, atuais ou livres de erros."
                      </p>
                  )
              )
    
    
              card(
                  card_header(card_title("4. Pagamentos"))
                  card_content(
                      <p class="text-sm leading-relaxed text-muted-foreground">
                          "Aceitamos pagamentos via PIX e cartão de crédito. O processamento do pagamento é realizado por terceiros e está sujeito aos termos e condições desses provedores de pagamento."
                      </p>
                  )
              )
    
    
              card(
                  card_header(card_title("5. Envio e Entrega"))
                  card_content(
                      <p class="text-sm leading-relaxed text-muted-foreground">
                          "O prazo de entrega varia de acordo com a localização e o método de envio selecionado. Não nos responsabilizamos por atrasos causados por terceiros, como transportadoras e correios."
                      </p>
                  )
              )
    
    
              card(
                  card_header(card_title("6. Trocas e Devoluções"))
                  card_content(
                      <p class="text-sm leading-relaxed text-muted-foreground">
                          "Você pode solicitar troca ou devolução em até 7 dias corridos após o recebimento do pedido, conforme o Código de Defesa do Consumidor. Consulte nossa página de Trocas e Devoluções para mais detalhes."
                      </p>
                  )
              )
    
    
              card(
                  card_header(card_title("7. Limitação de Responsabilidade"))
                  card_content(
                      <p class="text-sm leading-relaxed text-muted-foreground">
                          "Em nenhuma circunstância a Linda Flor será responsável por quaisquer danos diretos, indiretos, incidentais, especiais ou consequenciais que resultem do uso ou da incapacidade de usar este site ou seus conteúdos."
                      </p>
                  )
              )
    
      )
  })
}
