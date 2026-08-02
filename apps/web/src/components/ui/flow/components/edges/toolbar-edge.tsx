import {
  BaseEdge,
  EdgeToolbar,
  getBezierPath,
  type EdgeProps,
} from "@xyflow/react";
import type { ReactNode } from "react";

export type ToolbarEdgeProps = EdgeProps & {
  children?: ReactNode;
};

export function ToolbarEdge({
  id,
  sourceX,
  sourceY,
  targetX,
  targetY,
  sourcePosition,
  targetPosition,
  selected,
  style,
  markerEnd,
  children,
}: ToolbarEdgeProps) {
  const [edgePath] = getBezierPath({
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
      <EdgeToolbar
        edgeId={id}
        isVisible={selected}
        x={(sourceX + targetX) / 2}
        y={(sourceY + targetY) / 2}
      >
        {children}
      </EdgeToolbar>
    </>
  );
}
