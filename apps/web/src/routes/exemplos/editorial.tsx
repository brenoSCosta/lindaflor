import { createFileRoute } from "@tanstack/react-router";

import { EditorialHomeExample } from "@/routes/exemplos/-components/editorial-home-example";
import { ExampleShell } from "@/routes/exemplos/-components/example-shell";

export const Route = createFileRoute("/exemplos/editorial")({
  component: EditorialExamplePage,
  head: () => ({
    meta: [{ title: "Exemplo Editorial — Linda Flor" }],
  }),
});

function EditorialExamplePage() {
  return (
    <ExampleShell
      current="editorial"
      title="Editorial Playfair"
      subtitle="Homepage · hero full-bleed"
    >
      <EditorialHomeExample />
    </ExampleShell>
  );
}
