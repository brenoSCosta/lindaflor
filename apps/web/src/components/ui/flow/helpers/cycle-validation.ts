import {
  getOutgoers,
  type Connection,
  type Edge,
  type Node,
} from "@xyflow/react";

export function isAcyclicConnection(
  connection: Connection | Edge,
  nodes: Node[],
  edges: Edge[],
) {
  const target = nodes.find((n) => n.id === connection.target);
  if (!target) return false;
  if (target.id === connection.source) return false;

  const hasCycle = (node: Node, visited = new Set<string>()): boolean => {
    if (visited.has(node.id)) return false;
    visited.add(node.id);
    for (const outgoer of getOutgoers(node, nodes, edges)) {
      if (outgoer.id === connection.source) return true;
      if (hasCycle(outgoer, visited)) return true;
    }
    return false;
  };

  return !hasCycle(target);
}
