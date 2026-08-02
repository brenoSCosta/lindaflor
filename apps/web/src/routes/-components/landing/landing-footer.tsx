import { Link } from "@tanstack/react-router";

import { assets } from "@/routes/-components/landing/assets";
import {
  brand,
  contact,
  legalLinks,
  nav,
  sectors,
} from "@/routes/-components/landing/content";
import { InstagramIcon } from "@/routes/-components/landing/icons/instagrem";
import { LinkedinIcon } from "@/routes/-components/landing/icons/linkedin";
import { Reveal } from "@/routes/-components/landing/reveal";

export function LandingFooter() {
  const year = new Date().getFullYear();

  return (
    <footer className="border-t border-border bg-card">
      <div className="bp-ruler h-1.5 w-full" />
      <Reveal className="mx-auto w-full max-w-360 px-6">
        <div className="grid grid-cols-2 gap-10 py-14 md:grid-cols-4 md:gap-8">
          <div className="col-span-2 md:col-span-1">
            <Link to="/" className="flex items-center gap-3">
              <span className="flex size-11 items-center justify-center rounded-md border border-border bg-background">
                <img src={assets.logoIcon} alt="" className="size-8" />
              </span>
              <span className="text-lg font-bold tracking-tight text-foreground">
                OG <span className="text-secondary">Service</span>
              </span>
            </Link>
            <p className="mt-4 max-w-xs text-sm leading-relaxed text-muted-foreground">
              {brand.slogan}
            </p>
            <div className="mt-4 flex items-center gap-2 font-mono text-xs tracking-[0.18em] text-muted-foreground uppercase">
              <span className="size-1.5 rotate-45 bg-secondary" />
              Operação &amp; Manutenção Industrial
            </div>
          </div>

          <nav aria-label="Navegação do rodapé">
            <h3 className="font-mono text-xs tracking-[0.2em] text-muted-foreground uppercase">
              Navegação
            </h3>
            <ul className="mt-4 space-y-2.5">
              {nav.map((item) => (
                <li key={item.href}>
                  <a
                    className="text-sm text-foreground transition-colors hover:text-secondary"
                    href={item.href}
                  >
                    {item.label}
                  </a>
                </li>
              ))}
            </ul>
          </nav>

          <div className="order-last col-span-2 md:order-0 md:col-span-1">
            <h3 className="font-mono text-xs tracking-[0.2em] text-muted-foreground uppercase">
              Contato
            </h3>
            <ul className="mt-4 space-y-2.5">
              <li>
                <a
                  className="text-sm wrap-break-word text-foreground transition-colors hover:text-secondary"
                  href={`mailto:${contact.email}`}
                >
                  {contact.email}
                </a>
              </li>
              <li>
                <a
                  className="text-sm text-foreground transition-colors hover:text-secondary"
                  href={`https://wa.me/${contact.whatsapp}`}
                  target="_blank"
                  rel="noopener noreferrer"
                >
                  {contact.phone}
                </a>
              </li>
            </ul>
            <div className="mt-5 flex items-center gap-2">
              <a
                className="flex size-9 items-center justify-center rounded-md border border-border bg-background transition-colors hover:border-secondary"
                href={contact.instagram}
                target="_blank"
                rel="noopener noreferrer external"
                aria-label="Instagram"
              >
                <InstagramIcon className="size-5" />
              </a>
              <a
                className="flex size-9 items-center justify-center rounded-md border border-border bg-background transition-colors hover:border-secondary"
                href={contact.linkedin}
                target="_blank"
                rel="noopener noreferrer external"
                aria-label="LinkedIn"
              >
                <LinkedinIcon className="size-5" />
              </a>
            </div>
          </div>

          <div>
            <h3 className="font-mono text-xs tracking-[0.2em] text-muted-foreground uppercase">
              Setores
            </h3>
            <ul className="mt-4 space-y-2.5">
              {sectors.map((sector) => (
                <li key={sector.name} className="text-sm text-muted-foreground">
                  {sector.short}
                </li>
              ))}
            </ul>
          </div>
        </div>

        <div className="flex flex-col gap-4 border-t border-border py-6 md:flex-row md:items-center md:justify-between">
          <p className="text-xs text-muted-foreground">
            © {year} {brand.legalName} · Consultoria Industrial.
          </p>
          <div className="flex flex-wrap gap-x-5 gap-y-2">
            {legalLinks.map((link) => (
              <a
                key={link.label}
                className="text-xs text-muted-foreground transition-colors hover:text-secondary"
                href={link.href}
              >
                {link.label}
              </a>
            ))}
          </div>
        </div>
      </Reveal>
    </footer>
  );
}
