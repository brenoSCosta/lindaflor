import { useQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { ArrowRight, Briefcase, FileText, MapPin, Users } from "lucide-react";

import { buttonVariants } from "@/components/ui/button";
import { orpc } from "@/lib/orpc";
import { cn } from "@/lib/utils";
import { Reveal } from "@/routes/-components/landing/reveal";
import { SectionHeading } from "@/routes/-components/landing/section-heading";

const highlights = [
  {
    icon: FileText,
    title: "Envie seu currículo",
    description:
      "Cadastre-se gratuitamente em poucos minutos e fique no radar dos nossos recrutadores.",
  },
  {
    icon: Users,
    title: "Conexão com recrutadores",
    description:
      "Seu perfil é avaliado pela equipe de Talentos da OG Service para oportunidades compatíveis.",
  },
  {
    icon: Briefcase,
    title: "Oportunidades industriais",
    description:
      "Conecte-se a vagas nas áreas de operação, manutenção, inspeção, engenharia e tecnologia.",
  },
];

export function TalentPoolSection() {
  const { data: careers = [] } = useQuery(
    orpc.career.v1.list.queryOptions({
      input: { onlyActive: true },
      select: (result) => result.data,
    }),
  );

  return (
    <section
      id="talentos"
      className="relative mx-auto w-full max-w-360 overflow-hidden px-6 py-24 md:py-28"
    >
      <div className="absolute inset-0 -z-10 bg-secondary/5" />
      <div className="absolute top-0 right-0 -z-10 hidden size-96 translate-x-1/3 -translate-y-1/4 rounded-full bg-secondary/10 blur-3xl md:block" />

      <SectionHeading
        eyebrow="Trabalhe Conosco"
        title="Faça parte do nosso banco de talentos"
        lead="Estamos sempre em busca de profissionais qualificados para atuar nos setores de óleo e gás, petroquímica, agronegócio, alimentos e logística industrial."
        align="center"
      />

      <div className="mt-14 grid gap-6 sm:grid-cols-2 lg:grid-cols-3">
        {highlights.map((item, index) => (
          <Reveal
            key={item.title}
            delay={80 * (index + 1)}
            className="group rounded-xl border border-border bg-card p-6 transition-colors hover:border-secondary"
          >
            <div className="mb-4 inline-flex rounded-lg bg-secondary/10 p-3 text-secondary">
              <item.icon className="size-6" />
            </div>
            <h3 className="font-display text-lg font-semibold text-foreground">
              {item.title}
            </h3>
            <p className="mt-2 text-sm leading-relaxed text-muted-foreground">
              {item.description}
            </p>
          </Reveal>
        ))}
      </div>

      {careers.length > 0 && (
        <div className="mt-16">
          <Reveal>
            <div className="flex items-center gap-3 font-mono text-xs tracking-[0.22em] uppercase text-muted-foreground">
              <span className="h-px w-8 bg-border" />
              <span>Vagas abertas</span>
            </div>
          </Reveal>

          <div className="mt-6 grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
            {careers.slice(0, 6).map((career, index) => (
              <Reveal key={career.id} delay={80 * (index + 1)}>
                <Link
                  to="/curriculum/submit"
                  search={{ career: career.id }}
                  className="group flex h-full flex-col justify-between rounded-xl border border-border bg-card p-5 transition-all duration-300 hover:-translate-y-1 hover:border-secondary hover:shadow-lg hover:shadow-secondary/5"
                >
                  <div>
                    <div className="mb-3 flex items-center gap-2 text-xs text-muted-foreground">
                      <span className="rounded-full bg-secondary/10 px-2 py-0.5 font-mono text-[0.625rem] tracking-wider uppercase text-secondary">
                        {career.type}
                      </span>
                      <span className="flex items-center gap-1">
                        <MapPin className="size-3" />
                        {career.location}
                      </span>
                    </div>
                    <h3 className="font-display text-base font-semibold text-foreground">
                      {career.title}
                    </h3>
                    <p className="mt-1 text-sm text-muted-foreground">
                      {career.department}
                    </p>
                  </div>
                  <span className="mt-4 inline-flex items-center justify-center gap-1 text-sm font-medium text-primary transition-colors group-hover:text-secondary/80">
                    Candidatar-se
                    <ArrowRight className="size-3.5 transition-transform group-hover:translate-x-0.5" />
                  </span>
                </Link>
              </Reveal>
            ))}
          </div>
        </div>
      )}

      <Reveal
        className="mt-14 flex flex-col items-center justify-center gap-4 sm:flex-row"
        delay={340}
      >
        <Link
          to="/curriculum/submit"
          className={cn(
            buttonVariants({ size: "lg" }),
            "min-w-44 gap-2 bg-primary font-mono text-xs font-semibold tracking-wider uppercase text-primary-foreground hover:bg-primary/85",
          )}
        >
          Enviar currículo
          <ArrowRight className="size-4" />
        </Link>
      </Reveal>
    </section>
  );
}
