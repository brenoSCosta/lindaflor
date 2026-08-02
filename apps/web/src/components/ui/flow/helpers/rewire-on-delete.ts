import { getConnectedEdges, type Edge, type Node } from "@xyflow/react";

export function deleteMiddleNode(
  middleId: string,
  nodes: Node[],
  edges: Edge[],
): { nodes: Node[]; edges: Edge[] } {
  const middle = nodes.find((n) => n.id === middleId);
  if (!middle) return { nodes, edges };

  const connected = getConnectedEdges([middle], edges);
  const incoming = connected.filter((e) => e.target === middleId);
  const outgoing = connected.filter((e) => e.source === middleId);

  const nextEdges = edges.filter(
    (e) => e.source !== middleId && e.target !== middleId,
  );

  for (const inn of incoming) {
    for (const out of outgoing) {
      nextEdges.push({
        id: `${inn.source}->${out.target}`,
        source: inn.source,
        target: out.target,
      });
    }
  }

  return {
    nodes: nodes.filter((n) => n.id !== middleId),
    edges: nextEdges,
  };
}
