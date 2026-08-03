import { Link } from "@tanstack/react-router";

import { StoreLogo } from "@/components/store/store-logo";
import { cn } from "@/lib/utils";

type AuthLayoutProps = {
  children: React.ReactNode;
  title?: string;
  subtitle?: string;
};

export function AuthLayout({ children, title, subtitle }: AuthLayoutProps) {
  return (
    <div className="store min-h-svh bg-(--lf-cream)">
      <div className="grid min-h-svh lg:grid-cols-2">
        <div className="relative hidden overflow-hidden lg:block">
          <img
            src="/products/biquini-canelado-rosa.jpg"
            alt=""
            className="absolute inset-0 h-full w-full object-cover"
          />
          <div className="absolute inset-0 bg-linear-to-t from-[color-mix(in_oklab,var(--lf-ink)_35%,transparent)] via-transparent to-transparent" />
          <div className="absolute inset-x-0 bottom-0 p-12">
            <p className="font-display text-3xl leading-tight text-white">
              Moda praia
              <br />
              com elegância
            </p>
            <p className="mt-3 max-w-xs text-sm text-white/75">
              Peças exclusivas para quem vive o verão com estilo.
            </p>
          </div>
        </div>

        <div className="flex flex-col px-6 py-10 sm:px-12 lg:px-16 lg:py-16">
          <div className="mb-10 flex items-center justify-between">
            <StoreLogo />
            <Link
              to="/"
              className="text-[10px] tracking-[0.18em] text-(--lf-muted) uppercase transition-colors hover:text-(--lf-pink)"
            >
              Voltar à loja
            </Link>
          </div>

          <div className="mx-auto flex w-full max-w-sm flex-1 flex-col justify-center">
            {title ? (
              <header className="mb-8 text-center lg:text-left">
                <h1 className="lf-heading text-3xl md:text-4xl">{title}</h1>
                {subtitle ? (
                  <p className="mt-2 text-sm text-(--lf-muted)">{subtitle}</p>
                ) : null}
              </header>
            ) : null}
            {children}
          </div>
        </div>
      </div>
    </div>
  );
}

export function AuthDivider() {
  return (
    <div className="relative my-6">
      <div className="absolute inset-0 flex items-center">
        <span className="w-full border-t border-(--lf-line)" />
      </div>
      <div className="relative flex justify-center">
        <span className="bg-(--lf-cream) px-3 text-[10px] tracking-[0.2em] text-(--lf-muted) uppercase">
          ou
        </span>
      </div>
    </div>
  );
}

export function AuthLink({
  className,
  ...props
}: React.ComponentProps<typeof Link>) {
  return (
    <Link
      className={cn(
        "text-sm text-(--lf-pink) transition-colors hover:text-(--lf-pink-deep)",
        className,
      )}
      {...props}
    />
  );
}
