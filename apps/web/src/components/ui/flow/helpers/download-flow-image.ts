import { getNodesBounds, getViewportForBounds, type Node } from "@xyflow/react";
import { Effect } from "effect";
import { toPng } from "html-to-image";

export type DownloadFlowImageOptions = {
  nodes: Node[];
  width?: number;
  height?: number;
  backgroundColor?: string;
  filename?: string;
  minZoom?: number;
  maxZoom?: number;
  padding?: number;
};

function resolveCssColor(value: string): string {
  if (!value.startsWith("var(")) {
    return value;
  }
  const probe = document.createElement("div");
  probe.style.color = value;
  document.body.appendChild(probe);
  const resolved = getComputedStyle(probe).color;
  probe.remove();
  return resolved || value;
}

export function downloadFlowImage({
  nodes,
  width = 800,
  height = 500,
  backgroundColor = "var(--background)",
  filename = "flow.png",
  minZoom = 0.5,
  maxZoom = 2,
  padding = 0.1,
}: DownloadFlowImageOptions) {
  return Effect.gen(function* () {
    const nodesBounds = getNodesBounds(nodes);
    const viewport = getViewportForBounds(
      nodesBounds,
      width,
      height,
      minZoom,
      maxZoom,
      padding,
    );
    const el = document.querySelector(".react-flow__viewport");
    if (!(el instanceof HTMLElement)) {
      return;
    }

    const resolvedBackground = resolveCssColor(backgroundColor);

    const dataUrl = yield* Effect.tryPromise({
      try: () =>
        toPng(el, {
          backgroundColor: resolvedBackground,
          width,
          height,
          style: {
            width: `${width}px`,
            height: `${height}px`,
            transform: `translate(${viewport.x}px, ${viewport.y}px) scale(${viewport.zoom})`,
          },
        }),
      catch: (cause) => new Error("Failed to export flow image", { cause }),
    });

    const a = document.createElement("a");
    a.href = dataUrl;
    a.download = filename;
    a.click();
  });
}
