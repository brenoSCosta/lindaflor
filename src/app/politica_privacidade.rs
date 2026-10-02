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
                  breadcrumb_item(breadcrumb_page("Política de Privacidade"))
              )
          )
          <h1 class="mt-6 text-4xl font-bold tracking-tight">"Política de Privacidade"</h1>
          <p class="mt-4 text-sm leading-relaxed text-muted-foreground">
              "A Linda Flor Moda Praia respeita sua privacidade. Coletamos apenas os dados necessários para processar pedidos, enviar comunicações sobre compras e melhorar sua experiência na loja."
          </p>
          <p class="mt-2 text-sm leading-relaxed text-muted-foreground">
              "Informações como nome, e-mail, endereço e telefone são utilizadas exclusivamente para entrega, suporte e cumprimento de obrigações legais. Não vendemos seus dados a terceiros."
          </p>
          <p class="mt-2 text-sm leading-relaxed text-muted-foreground">
              "Utilizamos cookies essenciais para manter sua sessão e preferências. Você pode solicitar acesso, correção ou exclusão dos seus dados entrando em contato pelo WhatsApp (79) 99816-5115."
          </p>
          separator(attrs: attributes! { class="my-8" })
          card(
              card_header(card_title("1. Coleta de Dados"))
              card_content(
                  <p class="text-sm leading-relaxed text-muted-foreground">
                      "Coletamos informações pessoais que você nos fornece voluntariamente ao realizar um pedido, criar uma conta ou entrar em contato conosco. Isso inclui nome, e-mail, endereço, telefone e informações de pagamento."
                  </p>
              )
          )
          <div class="mt-4">
              card(
                  card_header(card_title("2. Uso dos Dados"))
                  card_content(
                      <p class="text-sm leading-relaxed text-muted-foreground">
                          "Utilizamos seus dados para processar pedidos, enviar comunicações sobre compras, melhorar nossos produtos e serviços, e cumprir obrigações legais. Não utilizamos seus dados para fins de marketing sem seu consentimento."
                      </p>
                  )
              )
          </div>
          <div class="mt-4">
              card(
                  card_header(card_title("3. Compartilhamento de Dados"))
                  card_content(
                      <p class="text-sm leading-relaxed text-muted-foreground">
                          "Não vendemos, alugamos ou compartilhamos seus dados pessoais com terceiros, exceto quando necessário para processar pagamentos, realizar entregas ou cumprir obrigações legais."
                      </p>
                  )
              )
          </div>
          <div class="mt-4">
              card(
                  card_header(card_title("4. Cookies"))
                  card_content(
                      <p class="text-sm leading-relaxed text-muted-foreground">
                          "Utilizamos cookies essenciais para manter sua sessão e preferências. Você pode desativar os cookies nas configurações do seu navegador, mas isso pode afetar a funcionalidade do site."
                      </p>
                  )
              )
          </div>
          <div class="mt-4">
              card(
                  card_header(card_title("5. Seus Direitos"))
                  card_content(
                      <p class="text-sm leading-relaxed text-muted-foreground">
                          "Você tem direito a solicitar acesso, correção ou exclusão dos seus dados pessoais. Para exercer esses direitos, entre em contato conosco pelo WhatsApp (79) 99816-5115."
                      </p>
                  )
              )
          </div>
      </div>
  })
}
