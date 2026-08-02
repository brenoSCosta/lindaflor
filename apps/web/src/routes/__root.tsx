import type { QueryClient } from "@tanstack/react-query";
import { createRootRouteWithContext } from "@tanstack/react-router";

import { authClient } from "@/lib/auth-client";
import { orpc } from "@/lib/orpc";
import { Root } from "@/routes/-components/root";

import "@/index.css";
import "leaflet/dist/leaflet.css";

export interface RouterAppContext {
  orpc: typeof orpc;
  queryClient: QueryClient;
  auth: typeof authClient;
}

export const Route = createRootRouteWithContext<RouterAppContext>()({
  component: Root,
  head: () => ({
    meta: [
      { name: "referrer", content: "strict-origin-when-cross-origin" },
      {
        title: "Linda Flor — Moda Praia",
      },
      {
        name: "description",
        content:
          "Linda Flor Moda Praia — biquínis, maiôs e saídas de praia em Aracaju, SE. Compre online com entrega para todo o Brasil.",
      },
    ],
    links: [
      { rel: "icon", href: "/favicon.svg", type: "image/svg+xml" },
      { rel: "icon", href: "/favicon.ico", sizes: "any" },
      {
        rel: "icon",
        type: "image/png",
        sizes: "32x32",
        href: "/favicon-32x32.png",
      },
      {
        rel: "icon",
        type: "image/png",
        sizes: "16x16",
        href: "/favicon-16x16.png",
      },
      {
        rel: "apple-touch-icon",
        sizes: "180x180",
        href: "/apple-touch-icon.png",
      },
      { rel: "manifest", href: "/site.webmanifest" },
      { rel: "sitemap", type: "application/xml", href: "/sitemap.xml" },
    ],
  }),
});
