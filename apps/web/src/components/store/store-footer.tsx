import { Link } from "@tanstack/react-router";
import { MapPin, MessageCircle } from "lucide-react";

import { StoreLogo } from "@/components/store/store-logo";
import { InstagramIcon } from "@/routes/-components/landing/icons/instagrem";

export function StoreFooter() {
  return (
    <footer className="border-t border-(--lf-line) bg-(--lf-cream) text-(--lf-ink)">
      <div className="mx-auto grid max-w-7xl gap-12 px-4 py-16 md:grid-cols-5 md:px-8">
        <div className="space-y-4 md:col-span-2">
          <StoreLogo subline={false} />
          <p className="max-w-md text-sm leading-relaxed text-(--lf-muted)">
            Moda praia feminina com qualidade e atendimento próximo. Da praia de
            Aracaju para você brilhar em qualquer verão.
          </p>
          <p className="text-[10px] tracking-[0.18em] text-(--lf-gold) uppercase">
            @BIQUINISLINDAFLOR
          </p>
        </div>

        <div className="space-y-3">
          <h3 className="text-[10px] tracking-[0.2em] uppercase">Loja</h3>
          <div className="flex flex-col gap-2 text-sm text-(--lf-muted)">
            <Link to="/produtos" className="hover:text-(--lf-ink)">
              Catálogo
            </Link>
            <Link to="/colecoes" className="hover:text-(--lf-ink)">
              Coleções
            </Link>
            <Link
              to="/produtos"
              search={{ categoria: "biquini" }}
              className="hover:text-(--lf-ink)"
            >
              Biquínis
            </Link>
            <Link
              to="/produtos"
              search={{ categoria: "maio" }}
              className="hover:text-(--lf-ink)"
            >
              Maiôs
            </Link>
          </div>
        </div>

        <div className="space-y-3">
          <h3 className="text-[10px] tracking-[0.2em] uppercase">
            Institucional
          </h3>
          <div className="flex flex-col gap-2 text-sm text-(--lf-muted)">
            <Link to="/politica-privacidade" className="hover:text-(--lf-ink)">
              Privacidade
            </Link>
            <Link to="/termos" className="hover:text-(--lf-ink)">
              Termos de uso
            </Link>
            <Link to="/trocas-devolucoes" className="hover:text-(--lf-ink)">
              Trocas e devoluções
            </Link>
          </div>
        </div>

        <div className="space-y-3">
          <h3 className="text-[10px] tracking-[0.2em] uppercase">Contato</h3>
          <div className="space-y-3 text-sm text-(--lf-muted)">
            <a
              href="https://wa.me/5579998165115"
              target="_blank"
              rel="noreferrer"
              className="flex items-center gap-2 hover:text-(--lf-ink)"
            >
              <MessageCircle className="size-4" />
              (79) 99816-5115
            </a>
            <a
              href="https://instagram.com/BIQUINISLINDAFLOR"
              target="_blank"
              rel="noreferrer"
              className="flex items-center gap-2 hover:text-(--lf-ink)"
            >
              <InstagramIcon className="size-4" />
              @BIQUINISLINDAFLOR
            </a>
            <p className="flex items-start gap-2">
              <MapPin className="mt-0.5 size-4 shrink-0" />
              Rua Capitão Isaias Alves de Souza, 1100 · Aracaju, SE
            </p>
          </div>
        </div>
      </div>

      <div className="border-t border-(--lf-line) px-4 py-5 text-center text-[10px] tracking-[0.14em] text-(--lf-muted) uppercase">
        © {new Date().getFullYear()} Linda Flor Moda Praia
      </div>
    </footer>
  );
}
