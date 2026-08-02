import { FLOW_DEFAULT_NODE_SIZE } from "@/components/ui/flow/core/constants";

export function getNodeCenter(node: {
  internals: { positionAbsolute: { x: number; y: number } };
  measured: { width?: number; height?: number };
}) {
  const w = node.measured.width ?? FLOW_DEFAULT_NODE_SIZE.width;
  const h = node.measured.height ?? FLOW_DEFAULT_NODE_SIZE.height;
  return {
    x: node.internals.positionAbsolute.x + w / 2,
    y: node.internals.positionAbsolute.y + h / 2,
  };
}
