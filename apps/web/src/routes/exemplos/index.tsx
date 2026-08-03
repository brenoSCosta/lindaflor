import { createFileRoute, Link } from "@tanstack/react-router";
import { ArrowRight } from "lucide-react";

export const Route = createFileRoute("/exemplos/")({
  component: ExamplesHubPage,
  head: () => ({
    meta: [{ title: "Exemplos de design — Linda Flor" }],
  }),
});

const examples = [
  {
    to: "/exemplos/editorial",
    tag: "Homepage",
    title: "Editorial Playfair",
    description:
      "Hero full-bleed, tipografia serif grande, logo como wordmark — sem retângulo rosa. Tom tropical e quente.",
    mood: "MBM Swim · Zimmermann",
  },
  {
    to: "/exemplos/bodoni",
    tag: "Catálogo",
    title: "Bodoni Minimal",
    description:
      "Grid tipo revista de moda, filtros laterais, muito whitespace. Visual fashion/luxo.",
    mood: "Galeria · Boutique",
  },
  {
    to: "/exemplos/bossa-nova",
    tag: "Produto",
    title: "Bossa Nova",
    description:
      "PDP comercial e limpa: galeria, tamanhos, CTA rosa, acordeões e cross-sell.",
    mood: "Solid & Striped · Commerce",
  },
] as const;

function ExamplesHubPage() {
  return (
    <div className="min-h-screen bg-neutral-950 text-white">
      <div className="mx-auto max-w-4xl px-4 py-16 md:px-8 md:py-24">
        <p className="text-[10px] tracking-[0.24em] text-pink-300 uppercase">
          Design samples
        </p>
        <h1 className="mt-3 font-['Playfair_Display',serif] text-4xl md:text-5xl">
          Três direções para Linda Flor
        </h1>
        <p className="mt-4 max-w-2xl leading-relaxed text-white/65">
          Cada exemplo é uma página funcional com dados reais do catálogo.
          Compare estilos e escolha qual combina mais com a marca.
        </p>

        <div className="mt-12 space-y-4">
          {examples.map((example) => (
            <Link
              key={example.to}
              to={example.to}
              className="group flex items-center justify-between gap-6 border border-white/10 bg-white/5 p-6 transition-colors hover:border-pink-400/40 hover:bg-white/8"
            >
              <div>
                <p className="text-[10px] tracking-[0.18em] text-pink-300 uppercase">
                  {example.tag}
                </p>
                <h2 className="mt-1 text-xl font-medium">{example.title}</h2>
                <p className="mt-2 text-sm text-white/60">
                  {example.description}
                </p>
                <p className="mt-2 text-[10px] tracking-[0.14em] text-white/40 uppercase">
                  {example.mood}
                </p>
              </div>
              <ArrowRight className="size-5 shrink-0 text-white/40 transition-transform group-hover:translate-x-1 group-hover:text-pink-300" />
            </Link>
          ))}
        </div>

        <Link
          to="/"
          className="mt-10 inline-block text-sm text-white/50 underline underline-offset-4 hover:text-white"
        >
          Voltar ao site atual
        </Link>
      </div>
    </div>
  );
}
