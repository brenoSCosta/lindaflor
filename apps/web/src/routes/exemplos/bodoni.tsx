import { createFileRoute } from "@tanstack/react-router";

import { BodoniCatalogExample } from "@/routes/exemplos/-components/bodoni-catalog-example";
import { ExampleShell } from "@/routes/exemplos/-components/example-shell";

export const Route = createFileRoute("/exemplos/bodoni")({
  component: BodoniExamplePage,
  head: () => ({
    meta: [{ title: "Exemplo Bodoni — Linda Flor" }],
  }),
});

function BodoniExamplePage() {
  return (
    <ExampleShell
      current="bodoni"
      title="Bodoni Minimal"
      subtitle="Catálogo · fashion gallery"
    >
      <BodoniCatalogExample />
    </ExampleShell>
  );
}
