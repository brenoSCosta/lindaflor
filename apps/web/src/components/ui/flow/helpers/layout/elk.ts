import { Position, type Edge, type Node } from "@xyflow/react";
import ELK from "elkjs/lib/elk.bundled.js";

const elk = new ELK();

export type ElkLayoutOptions = {
  direction?: "DOWN" | "UP" | "RIGHT" | "LEFT";
  algorithm?: string;
  nodeNodeSpacing?: string;
  nodeWidth?: number;
  nodeHeight?: number;
  setHandlePositions?: boolean;
};

export async function layoutWithElk(
  nodes: Node[],
  edges: Edge[],
  {
    direction = "DOWN",
    algorithm = "layered",
    nodeNodeSpacing = "40",
    nodeWidth = 120,
    nodeHeight = 40,
    setHandlePositions = true,
  }: ElkLayoutOptions = {},
) {
  const graph = {
    id: "root",
    layoutOptions: {
      "elk.algorithm": algorithm,
      "elk.direction": direction,
      "elk.spacing.nodeNode": nodeNodeSpacing,
    },
    children: nodes.map((n) => ({
      id: n.id,
      width: nodeWidth,
      height: nodeHeight,
    })),
    edges: edges.map((e) => ({
      id: e.id,
      sources: [e.source],
      targets: [e.target],
    })),
  };
  const laid = await elk.layout(graph);
  return nodes.map((node) => {
    const child = laid.children?.find((c) => c.id === node.id);
    const next: Node = {
      ...node,
      position: { x: child?.x ?? 0, y: child?.y ?? 0 },
    };
    if (setHandlePositions) {
      if (direction === "RIGHT" || direction === "LEFT") {
        next.sourcePosition =
          direction === "RIGHT" ? Position.Right : Position.Left;
        next.targetPosition =
          direction === "RIGHT" ? Position.Left : Position.Right;
      } else {
        next.sourcePosition =
          direction === "DOWN" ? Position.Bottom : Position.Top;
        next.targetPosition =
          direction === "DOWN" ? Position.Top : Position.Bottom;
      }
    }
    return next;
  });
}
