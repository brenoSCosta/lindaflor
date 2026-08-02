import type { ReactNode } from "react";

import { AnnouncementBar } from "@/components/store/announcement-bar";
import { StoreFooter } from "@/components/store/store-footer";
import { StoreHeader } from "@/components/store/store-header";

type StoreLayoutProps = {
  children: ReactNode;
  headerVariant?: "light" | "solid" | "overlay";
};

export function StoreLayout({
  children,
  headerVariant = "light",
}: StoreLayoutProps) {
  return (
    <div className="store min-h-screen bg-[var(--lf-cream)] text-[var(--lf-ink)]">
      <AnnouncementBar />
      <StoreHeader variant={headerVariant} />
      {children}
      <StoreFooter />
    </div>
  );
}
