import { Link } from "@tanstack/react-router";

import { cn } from "@/lib/utils";

type StoreLogoProps = {
  className?: string;
  subline?: boolean;
};

export function StoreLogo({ className, subline = true }: StoreLogoProps) {
  return (
    <Link to="/" className={cn("inline-block text-center", className)}>
      <span className="font-display text-2xl italic md:text-3xl">
        Linda Flor
      </span>
      {subline ? (
        <span className="mt-0.5 block text-[9px] tracking-[0.28em] text-muted uppercase">
          Moda Praia
        </span>
      ) : null}
    </Link>
  );
}
