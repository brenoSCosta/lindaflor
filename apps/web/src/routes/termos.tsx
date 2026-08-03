import { createFileRoute } from "@tanstack/react-router";

import { StoreLayout } from "@/components/store/store-layout";

export const Route = createFileRoute("/termos")({
  component: TermsPage,
  head: () => ({
    meta: [{ title: "Termos de Uso — Linda Flor" }],
  }),
});

function TermsPage() {
  return (
    <StoreLayout headerVariant="solid">
      <article className="mx-auto max-w-3xl px-4 py-16 md:px-8">
        <h1 className="font-display text-5xl">Termos de Uso</h1>
        <div className="mt-8 space-y-4 text-sm leading-relaxed text-muted">
          <p>
            Ao utilizar o site da Linda Flor Moda Praia, você concorda com estes
            termos. Os preços, disponibilidade e descrições dos produtos podem
            ser alterados sem aviso prévio.
          </p>
          <p>
            Pedidos estão sujeitos à confirmação de pagamento e disponibilidade
            em estoque. Reservamo-nos o direito de cancelar pedidos em caso de
            inconsistências ou suspeita de fraude.
          </p>
          <p>
            Imagens são ilustrativas. Pequenas variações de cor podem ocorrer
            devido à calibração de tela e processo de fabricação têxtil.
          </p>
        </div>
      </article>
    </StoreLayout>
  );
}
