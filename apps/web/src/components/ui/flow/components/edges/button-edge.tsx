import {
  BaseEdge,
  EdgeLabelRenderer,
  getBezierPath,
  useReactFlow,
  type EdgeProps,
} from "@xyflow/react";
import type { ReactNode } from "react";

import { Button } from "@/components/ui/button";

export type ButtonEdgeProps = EdgeProps & {
  children?: ReactNode;
};

export function ButtonEdge({
  id,
  sourceX,
  sourceY,
  targetX,
  targetY,
  sourcePosition,
  targetPosition,
  style,
  markerEnd,
  children,
}: ButtonEdgeProps) {
  const { setEdges } = useReactFlow();
  const [edgePath, labelX, labelY] = getBezierPath({
    sourceX,
    sourceY,
    sourcePosition,
    targetX,
    targetY,
    targetPosition,
  });

  return (
    <>
      <BaseEdge id={id} path={edgePath} style={style} markerEnd={markerEnd} />
      <EdgeLabelRenderer>
        <div
          className="nodrag nopan absolute"
          style={{
            transform: `translate(-50%, -50%) translate(${labelX}px,${labelY}px)`,
          }}
        >
          {children ?? (
            <Button
              size="icon-xs"
              type="button"
              variant="destructive"
              onClick={() => setEdges((eds) => eds.filter((e) => e.id !== id))}
            >
              ×
            </Button>
          )}
        </div>
      </EdgeLabelRenderer>
    </>
  );
}
