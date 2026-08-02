import { useRef } from "react";

import { governance } from "@/routes/-components/landing/content";
import { Reveal } from "@/routes/-components/landing/reveal";
import { useBento } from "@/routes/-components/landing/use-bento";

export function GovernanceSection() {
  const gridRef = useRef<HTMLDivElement>(null);
  useBento(gridRef);

  return (
    <section id="governanca" className="mx-auto w-full max-w-360 px-6 py-20">
      <div className="relative overflow-hidden rounded-xl border border-border bg-card p-8 md:p-12">
        <div className="absolute top-0 right-0 hidden h-full w-1/3 bg-linear-to-l from-accent/25 to-transparent lg:block" />

        <div className="relative grid gap-12 lg:grid-cols-2">
          <Reveal className="space-y-4">
            <div className="flex items-center gap-2 text-secondary">
              <span className="size-2 animate-pulse rounded-full bg-secondary outline-2 outline-secondary/50" />
              <span className="text-sm font-semibold tracking-widest uppercase">
                Governança Corporativa
              </span>
            </div>
            <h2 className="text-3xl font-semibold tracking-tight text-card-foreground">
              Compromisso com a governança
            </h2>
            <p className="leading-relaxed text-muted-foreground">
              {governance.intro}
            </p>
          </Reveal>

          <div ref={gridRef} className="relative grid gap-4 sm:grid-cols-2">
            {governance.principles.map((principle) => {
              const Icon = principle.icon;
              return (
                <article
                  key={principle.title}
                  className="bento-card h-full rounded-lg border border-border bg-muted p-6"
                >
                  <div className="mb-3 flex items-center justify-between">
                    <Icon className="size-7 text-secondary" />
                    <span className="font-mono text-sm font-bold text-muted-foreground/60 transition-colors group-hover:text-secondary/70">
                      {principle.sigil}
                    </span>
                  </div>
                  <h3 className="mb-2 text-base font-semibold text-foreground transition-colors group-hover:text-secondary">
                    {principle.title}
                  </h3>
                  <p className="text-sm leading-relaxed text-muted-foreground">
                    {principle.body}
                  </p>
                </article>
              );
            })}
          </div>
        </div>
      </div>
    </section>
  );
}
