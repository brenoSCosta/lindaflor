import { useRef } from "react";

import type { Pillar } from "@/routes/-components/landing/content";
import { pillars } from "@/routes/-components/landing/content";
import { Reveal } from "@/routes/-components/landing/reveal";
import { SectionHeading } from "@/routes/-components/landing/section-heading";
import { useBorderGlow } from "@/routes/-components/landing/use-border-glow";

function PillarCard({ pillar, index }: { pillar: Pillar; index: number }) {
  const Icon = pillar.icon;
  const cardRef = useRef<HTMLElement>(null);
  useBorderGlow(cardRef);

  return (
    <Reveal
      ref={cardRef}
      as="article"
      delay={index * 110}
      className="group relative isolate flex flex-col rounded-lg border border-border bg-card p-7 transition-colors"
    >
      <span className="pointer-events-none absolute top-5 right-6 text-7xl font-bold text-muted-foreground/10 select-none">
        {String(index + 1).padStart(2, "0")}
      </span>

      <div className="flex items-center gap-3">
        <span className="flex size-12 items-center justify-center border border-border bg-muted transition-colors group-hover:border-secondary">
          <Icon className="size-6 text-secondary" />
        </span>
        <h3 className="text-2xl font-bold text-card-foreground">
          {pillar.label}
        </h3>
      </div>

      {pillar.body && (
        <p className="mt-3 leading-relaxed text-muted-foreground">
          {pillar.body}
        </p>
      )}

      {pillar.list && (
        <ul className="mt-4 space-y-2.5">
          {pillar.list.map((value) => (
            <li
              key={value}
              className="flex items-start gap-3 text-muted-foreground"
            >
              <span className="mt-2 size-1.5 shrink-0 rotate-45 bg-secondary" />
              <span>{value}</span>
            </li>
          ))}
        </ul>
      )}

      <div className="mt-auto flex items-center gap-2 border-t border-border pt-5 font-mono text-xs tracking-wide text-muted-foreground uppercase">
        <span className="text-secondary">↳</span>
        {pillar.note}
      </div>
    </Reveal>
  );
}

export function MissionVisionValuesSection() {
  return (
    <section
      id="principios"
      className="mx-auto w-full max-w-360 px-6 py-24 md:py-28"
    >
      <SectionHeading
        eyebrow="Princípios Norteadores"
        title="O que nos move e o que nos define"
        lead="A base que orienta cada decisão técnica, contrato e equipe em campo."
      />

      <div className="mt-12 grid gap-5 md:grid-cols-3">
        {pillars.map((pillar, index) => (
          <PillarCard key={pillar.code} pillar={pillar} index={index} />
        ))}
      </div>
    </section>
  );
}
