import { ArrowDown } from "lucide-react";

import { cssVar } from "@/lib/utils";
import { assets } from "@/routes/-components/landing/assets";
import { BlurText } from "@/routes/-components/landing/blur-text";
import {
  brand,
  sectors,
  type Sector,
} from "@/routes/-components/landing/content";
import { Marquee } from "@/routes/-components/landing/marquee";

const sloganWords = brand.slogan.split(" ");
const sloganBaseDelay = 240;
const sloganStep = 95;
const subheadDelay = sloganBaseDelay + sloganWords.length * sloganStep + 200;
const ctaDelay = subheadDelay + 150;
const metaDelay = ctaDelay + 150;

function SectorChip({ sector }: { sector: Sector }) {
  const Icon = sector.icon;
  return (
    <span className="flex items-center gap-2.5">
      <Icon className="size-4 text-secondary" />
      <span className="font-mono text-xs tracking-[0.18em] text-muted-foreground uppercase">
        {sector.short}
      </span>
    </span>
  );
}

export function HeroSection() {
  return (
    <section
      id="inicio"
      className="relative flex min-h-[93.3dvh] flex-col overflow-hidden"
    >
      <div
        className="pointer-events-none absolute inset-0 z-0"
        aria-hidden="true"
      >
        <img
          src={assets.background}
          alt=""
          className="size-full object-cover opacity-40"
        />
      </div>

      <div className="relative z-10 flex flex-1 items-center justify-center">
        <div className="relative mx-auto flex w-full max-w-4xl flex-col items-center px-4 text-center sm:px-12">
          <div
            className="hero-fade-in relative flex size-48 items-center justify-center md:size-92"
            style={cssVar("--fade-delay", "0ms")}
          >
            <img
              src={assets.logo}
              alt={`${brand.name} logo`}
              className="relative size-full object-contain"
            />
          </div>

          <div
            className="hero-fade-in mb-7 flex items-center gap-3.5 font-mono text-xs tracking-[0.28em] uppercase"
            style={cssVar("--fade-delay", "110ms")}
          >
            <span className="hero-sonar relative size-2 rounded-full bg-green-600 dark:bg-green-500" />
            <span className="font-bold text-primary dark:text-secondary">
              Operações Industriais
            </span>
            <span className="hidden font-semibold text-foreground sm:inline">
              em Setores Estratégicos
            </span>
          </div>

          <h1
            aria-label={sloganWords.join(" ")}
            className="mb-7 max-w-4xl text-5xl leading-[1.04] font-bold tracking-tight text-balance text-foreground sm:text-6xl lg:text-7xl"
          >
            <BlurText
              words={sloganWords}
              startDelay={sloganBaseDelay}
              delay={sloganStep}
            />
          </h1>

          <p
            className="hero-fade-in mb-9 max-w-2xl text-base leading-relaxed text-pretty text-foreground sm:text-lg"
            style={cssVar("--fade-delay", `${subheadDelay}ms`)}
          >
            Soluções integradas de operação e manutenção industrial para setores
            estratégicos. Alta disponibilidade, segurança operacional e
            desempenho contínuo em cada etapa do processo.
          </p>

          <div
            className="hero-fade-in flex w-full flex-col gap-3 sm:w-auto sm:flex-row"
            style={cssVar("--fade-delay", `${ctaDelay}ms`)}
          >
            <a
              className="inline-flex h-12 items-center justify-center rounded-lg bg-primary px-7 font-mono text-xs font-semibold tracking-wider text-primary-foreground uppercase transition-colors hover:bg-primary/85"
              href="#contato"
            >
              Falar com Consultor
            </a>
            <a
              className="inline-flex h-12 items-center justify-center rounded-lg border border-border bg-background px-7 font-mono text-xs font-semibold tracking-wider text-foreground uppercase transition-colors hover:bg-muted"
              href="#servicos"
            >
              Nossas Soluções
            </a>
          </div>
        </div>
      </div>

      <div
        className="hero-fade-in relative z-10 flex justify-center pb-5"
        style={cssVar("--fade-delay", `${metaDelay + 200}ms`)}
      >
        <span className="bp-scroll-hint flex flex-col items-center gap-1.5 font-mono text-[0.625rem] tracking-[0.25em] text-foreground uppercase">
          Rolar
          <ArrowDown className="size-3.5" />
        </span>
      </div>

      <div
        className="relative z-10 border-y border-border bg-card/70 py-3 backdrop-blur-sm"
        aria-hidden="true"
      >
        <Marquee
          items={sectors}
          gap={56}
          renderItem={(sector) => <SectorChip sector={sector} />}
        />
      </div>
    </section>
  );
}
