export type FlowBox = { x: number; y: number; w: number; h: number };

export function boxesOverlap(a: FlowBox, b: FlowBox) {
  return (
    a.x < b.x + b.w && a.x + a.w > b.x && a.y < b.y + b.h && a.y + a.h > b.y
  );
}

export function nodesOverlap(
  a: { position: { x: number; y: number } },
  b: { position: { x: number; y: number } },
  width: number,
  height: number,
) {
  return boxesOverlap(
    { x: a.position.x, y: a.position.y, w: width, h: height },
    { x: b.position.x, y: b.position.y, w: width, h: height },
  );
}

export function resolveOverlapPushRight(
  dragged: { position: { x: number; y: number } },
  other: { position: { x: number; y: number } },
  nodeWidth: number,
  gap = 16,
) {
  return {
    x: dragged.position.x + nodeWidth + gap,
    y: other.position.y,
  };
}

export function edgeBoundingBox(
  source: { position: { x: number; y: number } },
  target: { position: { x: number; y: number } },
  nodeW = 80,
  nodeH = 40,
): FlowBox {
  const minX = Math.min(source.position.x, target.position.x);
  const maxX = Math.max(source.position.x, target.position.x) + nodeW;
  const minY = Math.min(source.position.y, target.position.y);
  const maxY = Math.max(source.position.y, target.position.y) + nodeH;
  return {
    x: minX,
    y: minY,
    w: maxX - minX,
    h: Math.max(20, maxY - minY),
  };
}
