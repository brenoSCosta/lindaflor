import { graphlib, layout } from "@dagrejs/dagre";
import { Position, type Edge, type Node } from "@xyflow/react";

export type DagreLayoutOptions = {
  rankdir?: "TB" | "BT" | "LR" | "RL";
  nodesep?: number;
  ranksep?: number;
  nodeWidth?: number;
  nodeHeight?: number;
};

export function layoutWithDagre(
  nodes: Node[],
  edges: Edge[],
  {
    rankdir = "TB",
    nodesep = 40,
    ranksep = 60,
    nodeWidth = 120,
    nodeHeight = 40,
  }: DagreLayoutOptions = {},
) {
  const g = new graphlib.Graph().setDefaultEdgeLabel(() => ({}));
  g.setGraph({ rankdir, nodesep, ranksep });
  for (const node of nodes) {
    g.setNode(node.id, { width: nodeWidth, height: nodeHeight });
  }
  for (const edge of edges) {
    g.setEdge(edge.source, edge.target);
  }
  layout(g);
  const horizontal = rankdir === "LR" || rankdir === "RL";
  return nodes.map((node) => {
    const pos = g.node(node.id);
    return {
      ...node,
      position: {
        x: pos.x - nodeWidth / 2,
        y: pos.y - nodeHeight / 2,
      },
      sourcePosition: horizontal ? Position.Right : Position.Bottom,
      targetPosition: horizontal ? Position.Left : Position.Top,
    };
  });
}
