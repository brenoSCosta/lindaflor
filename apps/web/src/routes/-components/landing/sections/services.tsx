import { services } from "@/routes/-components/landing/content";
import { InteractiveSelector } from "@/routes/-components/landing/interactive-selector";
import { SectionHeading } from "@/routes/-components/landing/section-heading";

export function ServicesSection() {
  return (
    <section
      id="servicos"
      className="mx-auto w-full max-w-360 px-6 py-24 md:py-28"
    >
      <SectionHeading
        eyebrow="Capacidades Técnicas"
        title="Produtos e Serviços"
        lead="Seis frentes integradas que cobrem o ciclo completo de manutenção, confiabilidade e operação industrial."
        align="center"
      />
      <InteractiveSelector options={services} className="mt-14 md:h-150" />
    </section>
  );
}
