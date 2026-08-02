import { createFileRoute } from "@tanstack/react-router";

import { BossaNovaProductExample } from "@/routes/exemplos/-components/bossa-nova-product-example";
import { ExampleShell } from "@/routes/exemplos/-components/example-shell";

export const Route = createFileRoute("/exemplos/bossa-nova")({
  component: BossaNovaExamplePage,
  head: () => ({
    meta: [{ title: "Exemplo Bossa Nova — Linda Flor" }],
  }),
});

function BossaNovaExamplePage() {
  return (
    <ExampleShell
      current="bossa-nova"
      title="Bossa Nova"
      subtitle="Página de produto · commerce clean"
    >
      <BossaNovaProductExample />
    </ExampleShell>
  );
}
