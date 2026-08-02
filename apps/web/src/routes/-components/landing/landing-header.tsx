import { Link, useRouterState } from "@tanstack/react-router";
import { Menu } from "lucide-react";
import { useEffect, useMemo, useState } from "react";

import { ThemeToggle } from "@/components/theme-toggle";
import { Button } from "@/components/ui/button";
import { Sheet, SheetContent, SheetTrigger } from "@/components/ui/sheet";
import { cn } from "@/lib/utils";
import { assets } from "@/routes/-components/landing/assets";
import { brand, nav } from "@/routes/-components/landing/content";

export function LandingHeader() {
  const pathname = useRouterState({ select: (s) => s.location.pathname });
  const [mobileMenuOpen, setMobileMenuOpen] = useState(false);
  const [activeId, setActiveId] = useState("");
  const navLinkHref = useMemo(
    () => (href: string) => (pathname === "/" ? href : `/${href}`),
    [pathname],
  );
  const [scrolled, setScrolled] = useState(
    () => typeof document !== "undefined" && window.scrollY > 8,
  );

  useEffect(() => {
    const media = window.matchMedia("(min-width: 768px)");
    const handleChange = (event: MediaQueryListEvent) => {
      if (event.matches) setMobileMenuOpen(false);
    };

    media.addEventListener("change", handleChange);
    return () => media.removeEventListener("change", handleChange);
  }, []);

  useEffect(() => {
    const ids = nav.map((item) => item.href.replace("#", ""));
    const elements = ids
      .map((id) => document.getElementById(id))
      .filter((element): element is HTMLElement => element !== null);

    const observer = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (entry.isIntersecting) setActiveId(entry.target.id);
        }
      },
      { rootMargin: "-45% 0px -50% 0px" },
    );

    elements.forEach((element) => observer.observe(element));
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    const onScroll = () => setScrolled(window.scrollY > 8);
    window.addEventListener("scroll", onScroll, { passive: true });
    return () => window.removeEventListener("scroll", onScroll);
  }, []);

  return (
    <header className="sticky top-0 z-50 w-full">
      <div
        className={cn(
          "border-b transition-colors duration-300",
          scrolled
            ? "border-border/80 bg-background/85 backdrop-blur-md"
            : "border-transparent bg-background/40 backdrop-blur-sm",
        )}
      >
        <div className="mx-auto flex w-full max-w-360 items-center justify-between gap-6 px-6 py-3">
          <Link
            to="/"
            className="group flex items-center gap-3"
            aria-label={brand.name}
          >
            <span className="relative flex size-11 items-center justify-center rounded-md border border-border bg-card transition-colors group-hover:border-secondary">
              <img src={assets.logoIcon} alt="" className="size-8" />
            </span>
            <span className="flex flex-col leading-none">
              <span className="text-lg font-bold tracking-tight text-foreground">
                OG <span className="text-secondary">Service</span>
              </span>
              <span className="mt-1 font-mono text-[0.5625rem] tracking-[0.2em] text-muted-foreground uppercase">
                Operação Industrial
              </span>
            </span>
          </Link>

          <nav
            className="hidden items-center gap-9 lg:flex"
            aria-label="Navegação principal"
          >
            {nav.map((item) => {
              const id = item.href.replace("#", "");

              return (
                <a
                  key={item.href}
                  className="nav-link font-mono text-xs tracking-[0.12em] text-muted-foreground uppercase transition-colors duration-200 hover:text-foreground data-active:text-foreground"
                  href={navLinkHref(item.href)}
                  data-active={activeId === id ? "true" : undefined}
                >
                  {item.label}
                </a>
              );
            })}
          </nav>

          <div className="flex items-center gap-2">
            <div className="hidden md:flex items-center gap-2">
              <Button
                variant="link"
                nativeButton={false}
                className="hidden sm:inline-flex"
                render={
                  <a href={navLinkHref("#contato")}>Falar com Consultor</a>
                }
              />
              <ThemeToggle />
              <Button
                nativeButton={false}
                render={<Link to="/login">Entrar</Link>}
              />
            </div>
            <Sheet open={mobileMenuOpen} onOpenChange={setMobileMenuOpen}>
              <SheetTrigger
                render={(props) => (
                  <Button
                    variant="outline"
                    size="icon"
                    className="lg:hidden"
                    {...props}
                  >
                    <Menu className="size-4" />
                  </Button>
                )}
              />
              <SheetContent
                side="right"
                className="flex w-80 flex-col bg-background p-0"
              >
                <div className="flex items-center gap-3 border-b border-border px-6 py-5">
                  <img src={assets.logoIcon} alt="" className="size-8" />
                  <span className="text-base font-bold tracking-tight text-foreground">
                    OG <span className="text-secondary">Service</span>
                  </span>
                </div>
                <nav className="flex flex-col px-3 py-4">
                  {nav.map((item, index) => (
                    <a
                      key={item.href}
                      className="flex items-center gap-3 rounded-md p-3 text-base font-medium text-foreground transition-colors hover:bg-muted hover:text-secondary"
                      href={navLinkHref(item.href)}
                      onClick={() => setMobileMenuOpen(false)}
                    >
                      <span className="font-mono text-xs text-secondary">
                        {String(index + 1).padStart(2, "0")}
                      </span>
                      {item.label}
                    </a>
                  ))}
                </nav>
                <div className="mt-auto flex flex-col gap-4 border-t border-border px-6 py-5">
                  <div className="flex items-center justify-between">
                    <span className="font-mono text-xs tracking-wider text-muted-foreground uppercase">
                      Tema
                    </span>
                    <ThemeToggle />
                  </div>
                  <a
                    href={navLinkHref("#contato")}
                    onClick={() => setMobileMenuOpen(false)}
                    className="flex h-9 w-full items-center justify-center rounded-lg bg-primary px-2.5 font-mono text-xs font-semibold tracking-wider uppercase text-primary-foreground transition-colors hover:bg-primary/85"
                  >
                    Falar com Consultor
                  </a>
                  <Button
                    variant="link"
                    nativeButton={false}
                    render={<Link to="/login">Entrar</Link>}
                  />
                </div>
              </SheetContent>
            </Sheet>
          </div>
        </div>
      </div>
    </header>
  );
}
