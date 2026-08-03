import { Link } from "@tanstack/react-router";
import { Menu, Search, ShoppingBag, User } from "lucide-react";
import { useState } from "react";

import { CartDrawer } from "@/components/store/cart-drawer";
import { StoreLogo } from "@/components/store/store-logo";
import { StoreSearchDialog } from "@/components/store/store-search-dialog";
import { Button } from "@/components/ui/button";
import {
  Sheet,
  SheetContent,
  SheetHeader,
  SheetTitle,
  SheetTrigger,
} from "@/components/ui/sheet";
import { useCart } from "@/context/cart";
import { cn } from "@/lib/utils";

const navLinks = [
  { label: "Início", to: "/" },
  { label: "Catálogo", to: "/produtos" },
  { label: "Coleções", to: "/colecoes" },
  { label: "Sobre", to: "/", hash: "sobre" },
] as const;

type StoreHeaderProps = {
  variant?: "light" | "solid" | "overlay";
};

export function StoreHeader({ variant = "light" }: StoreHeaderProps) {
  const [menuOpen, setMenuOpen] = useState(false);
  const [cartOpen, setCartOpen] = useState(false);
  const [searchOpen, setSearchOpen] = useState(false);
  const { itemCount } = useCart();

  return (
    <>
      <header
        className={cn(
          "sticky top-0 z-50 border-b backdrop-blur-md transition-colors",
          variant === "overlay"
            ? "border-white/10 bg-[color-mix(in_oklab,var(--lf-ink)_25%,transparent)] text-white"
            : variant === "light"
              ? "border-(--lf-line) bg-[color-mix(in_oklab,var(--lf-cream)_94%,white)]"
              : "border-(--lf-line) bg-(--lf-cream)",
        )}
      >
        <div className="mx-auto grid h-20 max-w-7xl grid-cols-[1fr_auto_1fr] items-center gap-4 px-4 md:px-8">
          <nav className="hidden items-center gap-6 md:flex">
            {navLinks.map((link) => (
              <Link
                key={link.label}
                to={link.to}
                {...("hash" in link ? { hash: link.hash } : {})}
                className={cn(
                  "text-[10px] tracking-[0.18em] uppercase transition-colors",
                  variant === "overlay"
                    ? "text-white/85 hover:text-white"
                    : "text-(--lf-ink) hover:text-(--lf-pink)",
                )}
              >
                {link.label}
              </Link>
            ))}
          </nav>

          <StoreLogo
            className={cn(
              variant === "overlay" && "text-white [&_span]:text-white",
            )}
          />

          <div className="flex items-center justify-end gap-1">
            <Button
              variant="ghost"
              size="icon"
              className={cn(
                "hidden md:inline-flex",
                variant === "overlay" &&
                  "text-white hover:bg-white/10 hover:text-white",
              )}
              aria-label="Buscar"
              onClick={() => setSearchOpen(true)}
            >
              <Search className="size-4" />
            </Button>
            <Link to="/conta/pedidos" className="hidden md:inline-flex">
              <Button
                variant="ghost"
                size="sm"
                className={cn(
                  "text-[10px] tracking-[0.14em] uppercase",
                  variant === "overlay" &&
                    "text-white hover:bg-white/10 hover:text-white",
                )}
              >
                Pedidos
              </Button>
            </Link>
            <Link to="/login" className="hidden md:inline-flex">
              <Button
                variant="ghost"
                size="icon"
                className={cn(
                  variant === "overlay" &&
                    "text-white hover:bg-white/10 hover:text-white",
                )}
              >
                <User className="size-4" />
              </Button>
            </Link>
            <Button
              variant="ghost"
              size="icon"
              className={cn(
                "relative",
                variant === "overlay" &&
                  "text-white hover:bg-white/10 hover:text-white",
              )}
              aria-label="Abrir carrinho"
              onClick={() => setCartOpen(true)}
            >
              <ShoppingBag className="size-4" />
              {itemCount > 0 ? (
                <span className="absolute -top-1 -right-1 flex size-4 items-center justify-center bg-(--lf-pink) text-[9px] text-white">
                  {itemCount > 9 ? "9+" : itemCount}
                </span>
              ) : null}
            </Button>

            <Sheet open={menuOpen} onOpenChange={setMenuOpen}>
              <SheetTrigger
                render={(props) => (
                  <Button
                    {...props}
                    variant="ghost"
                    size="icon"
                    className={cn(
                      "md:hidden",
                      variant === "overlay" &&
                        "text-white hover:bg-white/10 hover:text-white",
                    )}
                    aria-label="Abrir menu"
                  >
                    <Menu className="size-5" />
                  </Button>
                )}
              />
              <SheetContent
                side="right"
                className="store gap-0 border-(--lf-line) bg-(--lf-cream) p-0 shadow-none"
              >
                <SheetHeader className="border-b border-(--lf-line) px-6 pt-6 pb-5 pr-12">
                  <SheetTitle className="font-display text-2xl text-(--lf-ink)">
                    Menu
                  </SheetTitle>
                </SheetHeader>
                <nav className="flex flex-col gap-4 p-6">
                  {navLinks.map((link) => (
                    <Link
                      key={link.label}
                      to={link.to}
                      {...("hash" in link ? { hash: link.hash } : {})}
                      onClick={() => setMenuOpen(false)}
                      className="text-sm font-medium tracking-[0.14em] text-(--lf-ink) uppercase"
                    >
                      {link.label}
                    </Link>
                  ))}
                  <button
                    type="button"
                    onClick={() => {
                      setMenuOpen(false);
                      setSearchOpen(true);
                    }}
                    className="text-left text-sm font-medium tracking-[0.14em] text-(--lf-ink) uppercase"
                  >
                    Buscar
                  </button>
                  <Link
                    to="/login"
                    onClick={() => setMenuOpen(false)}
                    className="text-sm font-medium tracking-[0.14em] text-(--lf-pink) uppercase"
                  >
                    Entrar
                  </Link>
                </nav>
              </SheetContent>
            </Sheet>
          </div>
        </div>
      </header>

      <CartDrawer open={cartOpen} onOpenChange={setCartOpen} />
      <StoreSearchDialog open={searchOpen} onOpenChange={setSearchOpen} />
    </>
  );
}
