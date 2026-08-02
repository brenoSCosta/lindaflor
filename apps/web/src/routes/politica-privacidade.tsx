import { createFileRoute } from "@tanstack/react-router";

import { StoreLayout } from "@/components/store/store-layout";

export const Route = createFileRoute("/politica-privacidade")({
  component: PrivacyPage,
  head: () => ({
    meta: [{ title: "Política de Privacidade — Linda Flor" }],
  }),
});

function PrivacyPage() {
  return (
    <StoreLayout headerVariant="solid">
      <article className="mx-auto max-w-3xl px-4 py-16 md:px-8">
        <h1 className="font-display text-5xl">Política de Privacidade</h1>
        <div className="mt-8 space-y-4 text-sm leading-relaxed text-[var(--lf-muted)]">
          <p>
            A Linda Flor Moda Praia respeita sua privacidade. Coletamos apenas
            os dados necessários para processar pedidos, enviar comunicações
            sobre compras e melhorar sua experiência na loja.
          </p>
          <p>
            Informações como nome, e-mail, endereço e telefone são utilizadas
            exclusivamente para entrega, suporte e cumprimento de obrigações
            legais. Não vendemos seus dados a terceiros.
          </p>
          <p>
            Utilizamos cookies essenciais para manter sua sessão e preferências.
            Você pode solicitar acesso, correção ou exclusão dos seus dados
            entrando em contato pelo WhatsApp (79) 99816-5115.
          </p>
        </div>
      </article>
    </StoreLayout>
  );
}
