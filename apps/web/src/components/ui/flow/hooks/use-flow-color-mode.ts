import type { ColorMode } from "@xyflow/react";
import { useTheme } from "next-themes";

/**
 * Maps the app's resolved next-themes value to XYFlow's `colorMode`.
 * Falls back to `"light"` until the theme is resolved on the client.
 */
export function useFlowColorMode(): ColorMode {
  const { resolvedTheme } = useTheme();
  return resolvedTheme === "dark" ? "dark" : "light";
}
