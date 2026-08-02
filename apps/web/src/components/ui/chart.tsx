import { isRecord } from "effect/Predicate";
import React from "react";
import type { LegendProps } from "recharts";

import { cn } from "@/lib/utils";

// recharts is heavy, so its runtime components are code-split into their own
// async chunk via React.lazy (only the types above are statically imported).
// recharts locates `<Tooltip>` / `<Legend>` among a chart's children by their
// `displayName` (see recharts' findChildByType → getDisplayName), so the lazy
// wrappers must carry the same displayName to stay discoverable. A single
// <Suspense> around <ResponsiveContainer> (below) covers their resolution too,
// since the whole chart subtree renders inside it.
const ResponsiveContainer = React.lazy(() =>
  import("recharts").then((module) => ({
    default: module.ResponsiveContainer,
  })),
);

const Tooltip = Object.assign(
  React.lazy(() =>
    import("recharts").then((module) => ({ default: module.Tooltip })),
  ),
  { displayName: "Tooltip" },
);

const Legend = Object.assign(
  React.lazy(() =>
    import("recharts").then((module) => ({ default: module.Legend })),
  ),
  { displayName: "Legend" },
);

// Format: { THEME_NAME: CSS_SELECTOR }
const THEMES = { light: "", dark: ".dark" } as const;

export type ChartConfig = {
  [k in string]: {
    label?: React.ReactNode;
    icon?: React.ComponentType;
  } & (
    | { color?: string; theme?: never }
    | { color?: never; theme: Record<keyof typeof THEMES, string> }
  );
};

type ChartContextProps = {
  config: ChartConfig;
};

const ChartContext = React.createContext<ChartContextProps | null>(null);

function useChart() {
  const context = React.use(ChartContext);

  if (!context) {
    throw new Error("useChart must be used within a <ChartContainer />");
  }

  return context;
}

function ChartContainer({
  id,
  className,
  children,
  config,
  ...props
}: React.ComponentProps<"div"> & {
  config: ChartConfig;
  children: React.ComponentProps<typeof ResponsiveContainer>["children"];
}) {
  const uniqueId = React.useId();
  const chartId = `chart-${id || uniqueId.replace(/:/g, "")}`;

  const contextValue = React.useMemo(() => ({ config }), [config]);

  return (
    <ChartContext.Provider value={contextValue}>
      <div
        data-slot="chart"
        data-chart={chartId}
        className={cn(
          "flex aspect-video justify-center text-xs [&_.recharts-cartesian-axis-tick_text]:fill-muted-foreground [&_.recharts-cartesian-grid_line[stroke='#ccc']]:stroke-border/50 [&_.recharts-curve.recharts-tooltip-cursor]:stroke-border [&_.recharts-dot[stroke='#fff']]:stroke-transparent [&_.recharts-layer]:outline-hidden [&_.recharts-polar-grid_[stroke='#ccc']]:stroke-border [&_.recharts-radial-bar-background-sector]:fill-muted [&_.recharts-rectangle.recharts-tooltip-cursor]:fill-muted [&_.recharts-reference-line_[stroke='#ccc']]:stroke-border [&_.recharts-sector]:outline-hidden [&_.recharts-sector[stroke='#fff']]:stroke-transparent [&_.recharts-surface]:outline-hidden",
          className,
        )}
        {...props}
      >
        <ChartStyle id={chartId} config={config} />
        <React.Suspense fallback={null}>
          <ResponsiveContainer>{children}</ResponsiveContainer>
        </React.Suspense>
      </div>
    </ChartContext.Provider>
  );
}

const ChartStyle = ({ id, config }: { id: string; config: ChartConfig }) => {
  const colorConfig = Object.entries(config).filter(
    ([, itemConfig]) => itemConfig.theme || itemConfig.color,
  );

  if (!colorConfig.length) {
    return null;
  }

  return (
    <style>
      {(["light", "dark"] as const)
        .map((theme) => {
          const prefix = THEMES[theme];
          return `
${prefix} [data-chart=${id}] {
${colorConfig
  .map(([key, itemConfig]) => {
    const color = itemConfig.theme?.[theme] || itemConfig.color;
    return color ? `  --color-${key}: ${color};` : null;
  })
  .join("\n")}
}
`;
        })
        .join("\n")}
    </style>
  );
};

const ChartTooltip = Tooltip;

type ChartTooltipContentProps = React.ComponentProps<typeof Tooltip> &
  React.ComponentProps<"div"> & {
    hideLabel?: boolean;
    hideIndicator?: boolean;
    indicator?: "line" | "dot" | "dashed";
    nameKey?: string;
    labelKey?: string;
  };

function ChartTooltipLabel({
  config,
  payload,
  hideLabel,
  label,
  labelKey,
  labelFormatter,
  labelClassName,
}: Pick<
  ChartTooltipContentProps,
  | "payload"
  | "hideLabel"
  | "label"
  | "labelKey"
  | "labelFormatter"
  | "labelClassName"
> & {
  config: ChartConfig;
}) {
  if (hideLabel || !payload?.length) {
    return null;
  }

  const [item] = payload;
  const key = `${labelKey || item?.dataKey || item?.name || "value"}`;
  const itemConfig = getPayloadConfigFromPayload(config, item, key);
  const value =
    !labelKey && typeof label === "string"
      ? config[label]?.label || label
      : itemConfig?.label;

  if (labelFormatter) {
    return (
      <div className={cn("font-medium", labelClassName)}>
        {labelFormatter(value, payload)}
      </div>
    );
  }

  if (!value) {
    return null;
  }

  return <div className={cn("font-medium", labelClassName)}>{value}</div>;
}

function ChartTooltipContent({
  active,
  payload,
  className,
  indicator = "dot",
  hideLabel = false,
  hideIndicator = false,
  label,
  labelFormatter,
  labelClassName,
  formatter,
  color,
  nameKey,
  labelKey,
}: ChartTooltipContentProps) {
  const { config } = useChart();

  if (!active || !payload?.length) {
    return null;
  }

  const nestLabel = payload.length === 1 && indicator !== "dot";

  const tooltipLabel = (
    <ChartTooltipLabel
      config={config}
      payload={payload}
      hideLabel={hideLabel}
      label={label}
      labelKey={labelKey}
      labelFormatter={labelFormatter}
      labelClassName={labelClassName}
    />
  );

  const tooltipItems: React.ReactNode[] = [];
  for (const item of payload) {
    if (item.type === "none") {
      continue;
    }

    // Index within the kept items, matching the previous .filter().map() index.
    const index = tooltipItems.length;
    const key = `${nameKey || item.name || item.dataKey || "value"}`;
    const itemConfig = getPayloadConfigFromPayload(config, item, key);
    const indicatorColor = color || item.payload.fill || item.color;

    tooltipItems.push(
      <div
        key={item.dataKey}
        className={cn(
          "flex w-full flex-wrap items-stretch gap-2 [&>svg]:h-2.5 [&>svg]:w-2.5 [&>svg]:text-muted-foreground",
          indicator === "dot" && "items-center",
        )}
      >
        {formatter && item?.value !== undefined && item.name ? (
          formatter(item.value, item.name, item, index, item.payload)
        ) : (
          <>
            {itemConfig?.icon ? (
              <itemConfig.icon />
            ) : (
              !hideIndicator && (
                <div
                  className={cn(
                    "shrink-0 rounded-[2px] border-(--color-border) bg-(--color-bg)",
                    {
                      "h-2.5 w-2.5": indicator === "dot",
                      "w-1": indicator === "line",
                      "w-0 border-[1.5px] border-dashed bg-transparent":
                        indicator === "dashed",
                      "my-0.5": nestLabel && indicator === "dashed",
                    },
                  )}
                  style={
                    {
                      "--color-bg": indicatorColor,
                      "--color-border": indicatorColor,
                    } as React.CSSProperties & Record<string, string>
                  }
                />
              )
            )}
            <div
              className={cn(
                "flex flex-1 justify-between leading-none",
                nestLabel ? "items-end" : "items-center",
              )}
            >
              <div className="grid gap-1.5">
                {nestLabel ? tooltipLabel : null}
                <span className="text-muted-foreground">
                  {itemConfig?.label || item.name}
                </span>
              </div>
              {item.value && (
                <span className="font-mono font-medium text-foreground tabular-nums">
                  {item.value.toLocaleString()}
                </span>
              )}
            </div>
          </>
        )}
      </div>,
    );
  }

  return (
    <div
      className={cn(
        "grid min-w-32 items-start gap-1.5 rounded-lg border border-border/50 bg-background px-2.5 py-1.5 text-xs shadow-xl",
        className,
      )}
    >
      {!nestLabel ? tooltipLabel : null}
      <div className="grid gap-1.5">{tooltipItems}</div>
    </div>
  );
}

const ChartLegend = Legend;

function ChartLegendContent({
  className,
  hideIcon = false,
  payload,
  verticalAlign = "bottom",
  nameKey,
}: React.ComponentProps<"div"> &
  Pick<LegendProps, "payload" | "verticalAlign"> & {
    hideIcon?: boolean;
    nameKey?: string;
  }) {
  const { config } = useChart();

  if (!payload?.length) {
    return null;
  }

  const legendItems: React.ReactNode[] = [];
  for (const item of payload) {
    if (item.type === "none") {
      continue;
    }

    const key =
      nameKey || (item.dataKey != null ? String(item.dataKey) : "value");
    const itemConfig = getPayloadConfigFromPayload(config, item, key);

    legendItems.push(
      <div
        key={item.value}
        className={cn(
          "flex items-center gap-1.5 [&>svg]:h-3 [&>svg]:w-3 [&>svg]:text-muted-foreground",
        )}
      >
        {itemConfig?.icon && !hideIcon ? (
          <itemConfig.icon />
        ) : (
          <div
            className="size-2 shrink-0 rounded-[2px]"
            style={{
              backgroundColor: item.color,
            }}
          />
        )}
        {itemConfig?.label}
      </div>,
    );
  }

  return (
    <div
      className={cn(
        "flex items-center justify-center gap-4",
        verticalAlign === "top" ? "pb-3" : "pt-3",
        className,
      )}
    >
      {legendItems}
    </div>
  );
}
function getPayloadConfigFromPayload(
  config: ChartConfig,
  payload: unknown,
  key: string,
) {
  if (!isRecord(payload)) {
    return undefined;
  }

  const payloadPayload =
    "payload" in payload && isRecord(payload.payload)
      ? payload.payload
      : undefined;

  let configLabelKey: string = key;

  if (key in payload && typeof payload[key] === "string") {
    configLabelKey = payload[key];
  } else if (
    payloadPayload &&
    key in payloadPayload &&
    typeof payloadPayload[key] === "string"
  ) {
    configLabelKey = payloadPayload[key];
  }

  return configLabelKey in config ? config[configLabelKey] : config[key];
}

export {
  ChartContainer,
  ChartLegend,
  ChartLegendContent,
  ChartStyle,
  ChartTooltip,
  ChartTooltipContent,
};
