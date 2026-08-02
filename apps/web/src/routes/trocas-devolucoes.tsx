import { createFileRoute } from "@tanstack/react-router";

import { StoreLayout } from "@/components/store/store-layout";

export const Route = createFileRoute("/trocas-devolucoes")({
  component: ReturnsPage,
  head: () => ({
    meta: [{ title: "Trocas e Devoluções — Linda Flor" }],
  }),
});

function ReturnsPage() {
  return (
    <StoreLayout headerVariant="solid">
      <article className="mx-auto max-w-3xl px-4 py-16 md:px-8">
        <h1 className="font-display text-5xl">Trocas e Devoluções</h1>
        <div className="mt-8 space-y-4 text-sm leading-relaxed text-[var(--lf-muted)]">
          <p>
            Você pode solicitar troca ou devolução em até 7 dias corridos após
            o recebimento do pedido, conforme o Código de Defesa do Consumidor.
          </p>
          <p>
            A peça deve estar sem uso, com etiquetas e na embalagem original.
            Para iniciar o processo, entre em contato pelo WhatsApp (79)
            99816-5115 informando o número do pedido.
          </p>
          <p>
            O frete de devolução por arrependimento é por conta do cliente,
            exceto em casos de defeito ou erro no envio. Após análise, o
            reembolso será feito via PIX em até 10 dias úteis.
          </p>
        </div>
      </article>
    </StoreLayout>
  );
}
