import { Link } from "@tanstack/react-router";
import type { ReactNode } from "react";

import { cn } from "@/lib/utils";

const examples = [
  { to: "/exemplos/editorial", label: "Editorial", active: "editorial" },
  { to: "/exemplos/bodoni", label: "Bodoni", active: "bodoni" },
  { to: "/exemplos/bossa-nova", label: "Bossa Nova", active: "bossa-nova" },
] as const;

type ExampleShellProps = {
  children: ReactNode;
  current: (typeof examples)[number]["active"];
  title: string;
  subtitle: string;
};

export function ExampleShell({
  children,
  current,
  title,
  subtitle,
}: ExampleShellProps) {
  return (
    <div className="min-h-screen bg-neutral-950 text-white">
      <div className="border-b border-white/10 bg-neutral-950/95 backdrop-blur-md">
        <div className="mx-auto flex max-w-7xl flex-wrap items-center justify-between gap-3 px-4 py-3 md:px-8">
          <div>
            <p className="text-[10px] tracking-[0.22em] text-pink-300 uppercase">
              Mock de design
            </p>
            <p className="text-sm font-medium">{title}</p>
            <p className="text-xs text-white/55">{subtitle}</p>
          </div>
          <div className="flex flex-wrap items-center gap-2">
            {examples.map((example) => (
              <Link
                key={example.to}
                to={example.to}
                className={cn(
                  "border px-3 py-1.5 text-[10px] tracking-[0.14em] uppercase transition-colors",
                  current === example.active
                    ? "border-pink-400 bg-pink-500/20 text-pink-100"
                    : "border-white/15 text-white/70 hover:border-white/30 hover:text-white",
                )}
              >
                {example.label}
              </Link>
            ))}
            <Link
              to="/exemplos"
              className="border border-white/15 px-3 py-1.5 text-[10px] tracking-[0.14em] text-white/70 uppercase hover:text-white"
            >
              Todos
            </Link>
            <Link
              to="/"
              className="px-3 py-1.5 text-[10px] tracking-[0.14em] text-white/55 uppercase hover:text-white"
            >
              Site atual
            </Link>
          </div>
        </div>
      </div>
      {children}
    </div>
  );
}
