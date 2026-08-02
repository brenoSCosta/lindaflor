import type { Edge, Node } from "@xyflow/react";

export function nodeDistance(a: Node, b: Node) {
  const dx = a.position.x - b.position.x;
  const dy = a.position.y - b.position.y;
  return Math.hypot(dx, dy);
}

export function edgesForProximity(
  node: Node,
  nodes: Node[],
  existing: Edge[],
  proximity = 80,
): Edge[] {
  const next: Edge[] = [];
  for (const other of nodes) {
    if (other.id === node.id) continue;
    if (nodeDistance(node, other) >= proximity) continue;
    const id = `${node.id}-${other.id}`;
    if (
      existing.some(
        (e) =>
          e.id === id ||
          (e.source === node.id && e.target === other.id) ||
          next.some((n) => n.id === id),
      )
    ) {
      continue;
    }
    next.push({ id, source: node.id, target: other.id });
  }
  return next;
}
