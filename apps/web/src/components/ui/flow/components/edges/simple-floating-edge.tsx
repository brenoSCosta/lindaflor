import {
  BaseEdge,
  getStraightPath,
  useStore,
  type EdgeProps,
} from "@xyflow/react";

import { FLOW_DEFAULT_NODE_SIZE } from "@/components/ui/flow/core/constants";
import { closestSide } from "@/components/ui/flow/helpers/closest-side";

export function SimpleFloatingEdge({
  id,
  source,
  target,
  markerEnd,
  style,
}: EdgeProps) {
  const sourceNode = useStore((s) => s.nodeLookup.get(source));
  const targetNode = useStore((s) => s.nodeLookup.get(target));
  if (!sourceNode || !targetNode) return null;

  const sw = sourceNode.measured.width ?? FLOW_DEFAULT_NODE_SIZE.width;
  const sh = sourceNode.measured.height ?? FLOW_DEFAULT_NODE_SIZE.height;
  const tw = targetNode.measured.width ?? FLOW_DEFAULT_NODE_SIZE.width;
  const th = targetNode.measured.height ?? FLOW_DEFAULT_NODE_SIZE.height;
  const pts = closestSide(
    { x: sourceNode.position.x, y: sourceNode.position.y, w: sw, h: sh },
    { x: targetNode.position.x, y: targetNode.position.y, w: tw, h: th },
  );
  const [path] = getStraightPath({
    sourceX: pts.x,
    sourceY: pts.y,
    targetX: pts.tx,
    targetY: pts.ty,
  });

  return <BaseEdge id={id} path={path} markerEnd={markerEnd} style={style} />;
}
