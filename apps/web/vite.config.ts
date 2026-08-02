import path from "node:path";
import { fileURLToPath } from "node:url";

import tailwindcss from "@tailwindcss/vite";
import { tanstackRouter } from "@tanstack/router-plugin/vite";
import react from "@vitejs/plugin-react";
import dotenv from "dotenv";
import { defineConfig, loadEnv } from "vite";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const projectRoot = path.resolve(__dirname, "../..");
dotenv.config({ path: path.resolve(projectRoot, ".env"), quiet: true });

export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, process.cwd(), "");
  const isProd = mode === "production";

  return {
    plugins: [
      tailwindcss(),
      tanstackRouter({
        autoCodeSplitting: true,
        routeFileIgnorePattern: "^dev$",
        ...(isProd
          ? {
              generatedRouteTree: "./src/routeTree.prod.gen.ts",
            }
          : {}),
      }),
      react(),
    ],
    worker: {
      format: "es",
    },
    resolve: {
      alias: [
        ...(isProd
          ? [
              {
                find: "@/routeTree.gen",
                replacement: path.resolve(
                  __dirname,
                  "./src/routeTree.prod.gen.ts",
                ),
              },
            ]
          : []),
        {
          find: "@",
          replacement: path.resolve(__dirname, "./src"),
        },
      ],
    },
    optimizeDeps: {
      include: ["jspdf", "jspdf-autotable", "papaparse", "xlsx"],
    },
    server: {
      host: true,
      port: Number(env.VITE_WEB_PORT),
      proxy: {
        "/rpc": {
          target: env.VITE_SERVER_URL ?? "http://localhost:9020",
          changeOrigin: true,
        },
        "/api": {
          target: env.VITE_SERVER_URL ?? "http://localhost:9020",
          changeOrigin: true,
        },
        "/webhooks": {
          target: env.VITE_SERVER_URL ?? "http://localhost:9020",
          changeOrigin: true,
        },
        "/sitemap.xml": {
          target: env.VITE_SERVER_URL ?? "http://localhost:9020",
          changeOrigin: true,
        },
      },
    },
    build: {
      rolldownOptions: {
        output: {
          comments: false,
          codeSplitting: {
            groups: [
              { name: "tanstack-query-vendor", test: /@tanstack\/react-query/ },
              {
                name: "tanstack-router-vendor",
                test: /@tanstack\/react-router/,
              },
              { name: "tanstack-table-vendor", test: /@tanstack\/react-table/ },
              { name: "tanstack-form-vendor", test: /@tanstack\/react-form/ },
              { name: "tanstack-vendor", test: /@tanstack/ },
              { name: "orpc-vendor", test: /@orpc/ },
              { name: "sonner-vendor", test: /sonner/ },
              { name: "lucide-vendor", test: /lucide-react/ },
              { name: "base-ui-vendor", test: /@base-ui/ },
              { name: "ark-ui-vendor", test: /@ark-ui/ },
              { name: "casl-vendor", test: /@casl/ },
              { name: "dnd-vendor", test: /@dnd-kit/ },
              { name: "effect-vendor", test: /effect/ },
              { name: "tailwindcss-vendor", test: /@tailwindcss/ },
              { name: "themes-vendor", test: /next-themes/ },
              { name: "zod-vendor", test: /zod/ },
              { name: "date-fns-vendor", test: /date-fns/ },
              { name: "qrcode-vendor", test: /html5-qrcode|qrcode\.react/ },
              { name: "better-auth-vendor", test: /better-auth/ },
              { name: "usehooks-vendor", test: /usehooks-ts/ },
              { name: "cmdk-vendor", test: /cmdk/ },
              { name: "input-otp-vendor", test: /input-otp/ },
              { name: "react-day-picker-vendor", test: /react-day-picker/ },
              {
                name: "utils-vendor",
                test: /tailwind-merge|clsx|class-variance-authority/,
              },
              { name: "tw-animate-css-vendor", test: /tw-animate-css/ },
              { name: "env-core-vendor", test: /@t3-oss\/env-core/ },
              { name: "leaflet-vendor", test: /leaflet|react-leaflet/ },
              { name: "motion-vendor", test: /motion/ },
              {
                name: "markdown-vendor",
                test: /react-markdown|remark-gfm/,
              },
              { name: "papaparse-vendor", test: /papaparse/ },
              {
                name: "react-resizable-panels-vendor",
                test: /react-resizable-panels/,
              },
              { name: "react-vendor", test: /react-dom|\/react$/ },
              { name: "vendor", test: /node_modules/, entriesAware: true },
            ],
          },
        },
      },
      // Mermaid peg/abnf/ebnf/railroad diagrams share one langium module (~663KB).
      chunkSizeWarningLimit: 700,
    },
  };
});
