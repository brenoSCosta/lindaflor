import { Position, useStore, type NodeProps } from "@xyflow/react";
import type { ReactNode } from "react";

import { BaseHandle } from "@/components/ui/flow/components/handles/base-handle";
import { BaseNode } from "@/components/ui/flow/components/nodes/base-node";
import { flowNodeLabel } from "@/components/ui/flow/helpers/node-label";

export type ContextualZoomNodeProps = NodeProps & {
  zoomThreshold?: number;
  detail?: ReactNode;
  summary?: ReactNode;
};

export function ContextualZoomNode({
  data,
  zoomThreshold = 1,
  detail,
  summary,
}: ContextualZoomNodeProps) {
  const zoom = useStore((s) => s.transform[2]);
  return (
    <BaseNode className="px-3 py-2 shadow-sm">
      <div className="font-medium">{flowNodeLabel(data, "Node")}</div>
      {zoom > zoomThreshold
        ? (detail ?? (
            <p className="text-muted-foreground mt-1 text-xs">
              Detail visible when zoomed in
            </p>
          ))
        : (summary ?? (
            <p className="text-muted-foreground mt-1 text-xs">Zoom in…</p>
          ))}
      <BaseHandle type="target" position={Position.Left} />
      <BaseHandle type="source" position={Position.Right} />
    </BaseNode>
  );
}
