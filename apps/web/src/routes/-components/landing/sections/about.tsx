import { assets } from "@/routes/-components/landing/assets";
import { about, sectors } from "@/routes/-components/landing/content";
import { Reveal } from "@/routes/-components/landing/reveal";
import { SectionHeading } from "@/routes/-components/landing/section-heading";

export function AboutSection() {
  return (
    <section
      id="sobre"
      className="mx-auto w-full max-w-360 px-6 py-24 md:py-28"
    >
      <div className="grid gap-12 lg:grid-cols-12 lg:gap-16">
        <div className="lg:col-span-7">
          <SectionHeading
            eyebrow="Sobre a OG Service"
            title="Consultoria e operação industrial especializada"
            lead={about.intro}
          />
          <Reveal
            className="mt-6 space-y-4 leading-relaxed text-muted-foreground"
            delay={80}
          >
            <p>{about.body}</p>
            <p>{about.model}</p>
          </Reveal>

          <Reveal className="mt-8" delay={140}>
            <span className="font-mono text-xs tracking-[0.2em] text-muted-foreground uppercase">
              Setores atendidos
            </span>
            <div className="mt-3 flex flex-wrap gap-2">
              {sectors.map((sector) => (
                <span
                  key={sector.name}
                  className="flex items-center gap-2 border border-border bg-card px-3 py-1.5 font-mono text-xs tracking-wide text-foreground uppercase transition-colors hover:border-secondary"
                >
                  <span className="size-1.5 rotate-45 bg-secondary" />
                  {sector.name}
                </span>
              ))}
            </div>
          </Reveal>
        </div>

        <Reveal
          as="figure"
          className="relative h-full min-h-80 overflow-hidden rounded-lg border border-border bg-card lg:col-span-5"
          delay={160}
        >
          <img
            src={assets.banner}
            alt="Operações industriais da OG Service"
            loading="lazy"
            decoding="async"
            className="size-full object-cover"
          />
        </Reveal>
      </div>
    </section>
  );
}
