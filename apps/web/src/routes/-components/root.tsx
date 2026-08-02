import { env } from "@lindaflor/env/web";
import { TanStackDevtools } from "@tanstack/react-devtools";
import { formDevtoolsPlugin } from "@tanstack/react-form-devtools";
import { ReactQueryDevtoolsPanel } from "@tanstack/react-query-devtools";
import { HeadContent, Outlet } from "@tanstack/react-router";
import { TanStackRouterDevtoolsPanel } from "@tanstack/react-router-devtools";

import { Toaster } from "@/components/ui/sonner";
import { CartProvider } from "@/context/cart";
import { ThemeProvider } from "@/context/theme";
import { TimeFormatProvider } from "@/context/time-format";
import { TimezoneProvider } from "@/context/timezone";

export function Root() {
  return (
    <>
      <HeadContent />
      <ThemeProvider
        attribute="class"
        defaultTheme="light"
        disableTransitionOnChange
        storageKey="vite-ui-theme"
      >
        <TimezoneProvider>
          <TimeFormatProvider>
            <CartProvider>
              <Outlet />
              <Toaster richColors />
            </CartProvider>
          </TimeFormatProvider>
        </TimezoneProvider>
      </ThemeProvider>
      {env.VITE_NODE_ENV === "development" && (
        <TanStackDevtools
          config={{
            position: "bottom-left",
            panelLocation: "bottom",
            hideUntilHover: true,
          }}
          plugins={[
            formDevtoolsPlugin(),
            {
              name: "TanStack Query",
              render: <ReactQueryDevtoolsPanel />,
            },
            {
              name: "TanStack Router",
              render: <TanStackRouterDevtoolsPanel />,
            },
          ]}
        />
      )}
    </>
  );
}
