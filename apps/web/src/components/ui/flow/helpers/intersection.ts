import type { Node } from "@xyflow/react";

export function markIntersectingNodes(
  nodes: Node[],
  intersectingIds: Set<string>,
  activeClassName = "ring-2 ring-primary",
) {
  return nodes.map((n) => ({
    ...n,
    className: intersectingIds.has(n.id) ? activeClassName : "",
  }));
}
