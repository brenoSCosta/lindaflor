import type { FlowBox } from "@/components/ui/flow/helpers/collision";

export function closestSide(source: FlowBox, target: FlowBox) {
  const sx = source.x + source.w / 2;
  const sy = source.y + source.h / 2;
  const tx = target.x + target.w / 2;
  const ty = target.y + target.h / 2;
  const dx = tx - sx;
  const dy = ty - sy;
  if (Math.abs(dx) > Math.abs(dy)) {
    return dx > 0
      ? { x: source.x + source.w, y: sy, tx: target.x, ty }
      : { x: source.x, y: sy, tx: target.x + target.w, ty };
  }
  return dy > 0
    ? { x: sx, y: source.y + source.h, tx, ty: target.y }
    : { x: sx, y: source.y, tx, ty: target.y + target.h };
}
