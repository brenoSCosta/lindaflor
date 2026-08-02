import type { ReactNode } from "react";

import { cn } from "@/lib/utils";

type TankagesPageShellProps = {
  children: ReactNode;
  className?: string;
};

export function TankagesPageShell({
  children,
  className,
}: TankagesPageShellProps) {
  return (
    <div
      className={cn(
        "mx-auto flex w-full max-w-6xl flex-col gap-6 px-4 py-6 sm:px-6 lg:px-8",
        className,
      )}
    >
      {children}
    </div>
  );
}
