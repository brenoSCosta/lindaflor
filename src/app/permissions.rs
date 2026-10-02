use serde::Deserialize;
use topcoat::{
  Result,
  context::Cx,
  router::{content::Form, page},
  view::{View, view},
};

use crate::components::alert::{alert, alert_title};
use crate::components::badge::{BadgeVariant, badge};
use crate::components::button::{ButtonVariant, button};
use crate::components::card::{
  card, card_content, card_description, card_header, card_title,
};
use crate::components::input::input;
use crate::components::label::label;
use crate::components::select::select;
use crate::components::table::{
  table, table_body, table_cell, table_head, table_header, table_row,
};
use topcoat::view::attributes;

#[derive(Deserialize)]
pub struct InviteInput {
  #[allow(dead_code)]
  email: Option<String>,
  #[allow(dead_code)]
  role: Option<String>,
}

struct Member {
  name: &'static str,
  email: &'static str,
  role: &'static str,
  joined: &'static str,
}

fn role_variant(role: &str) -> BadgeVariant {
  match role {
    "owner" => BadgeVariant::Primary,
    "admin" => BadgeVariant::Secondary,
    _ => BadgeVariant::Outline,
  }
}

#[page([GET, POST] "/permissions")]
pub async fn page(
  cx: &Cx,
  body: Option<Form<InviteInput>>,
) -> Result<impl View> {
  let invited =
    body.is_some() && topcoat::router::request::method(cx).as_str() == "POST";

  let members = vec![
    Member {
      name: "Maria Silva",
      email: "maria@exemplo.com",
      role: "owner",
      joined: "01/01/2026",
    },
    Member {
      name: "João Santos",
      email: "joao@exemplo.com",
      role: "admin",
      joined: "15/03/2026",
    },
    Member {
      name: "Ana Costa",
      email: "ana@exemplo.com",
      role: "member",
      joined: "20/06/2026",
    },
  ];
  let member_count = members.len();

  Ok(view! {
      <div class="mx-auto max-w-7xl px-4 py-8 md:px-8">
          <h1 class="text-2xl font-bold">"Permissões"</h1>
          <p class="mt-1 mb-8 text-sm text-muted-foreground">
              "Veja o que você pode fazer e gerencie quem tem acesso à organização."
          </p>

          card(
              card_header(
                  card_title("Seu acesso")
                  card_description("O que você pode fazer com base no seu papel na organização.")
              )
              card_content(
                  <div class="flex items-center gap-3">
                      <div class="flex size-10 items-center justify-center rounded-full bg-primary/10 text-primary">
                          <span class="text-base">"🛡"</span>
                      </div>
                  </div>
                  <span class="text-sm text-muted-foreground">"Conectado como"</span>
                  <span class="text-sm font-medium">"Maria Silva"</span>
                  badge(variant: BadgeVariant::Primary, "Proprietário da org")
              )
          )

          <div class="my-6 overflow-hidden rounded-xl border border-border bg-background shadow-sm">
              <div class="flex items-center justify-between border-b border-border px-6 py-4">
                  <h2 class="font-semibold">"Membros da organização"</h2>
                  badge(variant: BadgeVariant::Secondary, (member_count))
              </div>
              table(
                  table_header(
                      table_row(
                          table_head("Membro")
                          table_head("E-mail")
                          table_head("Papel")
                          table_head("Entrou em")
                      )
                  )
                  table_body(
                      for member in members {
                          table_row(
                              table_cell(<span class="font-medium">(member.name)</span>)
                              table_cell(<span class="text-muted-foreground">(member.email)</span>)
                              table_cell(badge(variant: role_variant(member.role), (member.role)))
                              table_cell(<span class="text-muted-foreground">(member.joined)</span>)
                          )
                      }
                  )
              )
          </div>

          card(
              card_header(
                  card_title("Convidar membro")
                  card_description(
                      "Envie um e-mail de convite para adicionar alguém a esta organização."
                  )
              )
              card_content(
                  if invited {
                      alert(
                          alert_title("Convite enviado com sucesso.")
                      )
                  } else {
                      <form method="post" action="/permissions" class="flex flex-col gap-4">
                          <div class="space-y-2">
                              label(attrs: attributes! { for="email" }, "E-mail")
                              input(attrs: attributes! { type="email" name="email" id="email" placeholder="colega@exemplo.com" required="" })
                          </div>
                          <div class="space-y-2">
                              label(attrs: attributes! { for="role" }, "Papel")
                              select(
                                  attrs: attributes! { name="role" id="role" },
                                  <option value="member">"Membro"</option>
                                  <option value="admin">"Administrador"</option>
                                  <option value="owner">"Proprietário"</option>
                              )
                          </div>
                          button(
                              variant: ButtonVariant::Primary,
                              attrs: attributes! { type="submit" },
                              "Enviar convite"
                          )
                      </form>
                  }
              )
          )
      </div>
  })
}
