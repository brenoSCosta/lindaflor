import type { ColorMode, ReactFlowProps } from "@xyflow/react";
import type { ReactNode } from "react";

export type FlowColorMode = ColorMode;

export type FlowProps = Omit<ReactFlowProps, "colorMode"> & {
  /**
   * Defaults to the app theme from `next-themes` (`resolvedTheme`).
   * Pass explicitly to override (e.g. gallery demos with a local toggle).
   */
  colorMode?: FlowColorMode;
  children?: ReactNode;
};

export type FlowProviderProps = {
  children: ReactNode;
};
