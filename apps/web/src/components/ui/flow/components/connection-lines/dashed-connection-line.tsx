import type { ConnectionLineComponentProps } from "@xyflow/react";

export function DashedConnectionLine({
  fromX,
  fromY,
  toX,
  toY,
}: ConnectionLineComponentProps) {
  return (
    <g data-slot="flow-dashed-connection-line">
      <path
        fill="none"
        stroke="var(--primary)"
        strokeWidth={2}
        strokeDasharray="6 3"
        d={`M${fromX},${fromY} L${toX},${toY}`}
      />
      <circle
        cx={toX}
        cy={toY}
        fill="var(--background)"
        r={4}
        stroke="var(--primary)"
        strokeWidth={2}
      />
    </g>
  );
}
