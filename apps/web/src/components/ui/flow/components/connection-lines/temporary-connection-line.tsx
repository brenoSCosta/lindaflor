import type { ConnectionLineComponentProps } from "@xyflow/react";

export function TemporaryConnectionLine({
  fromX,
  fromY,
  toX,
  toY,
}: ConnectionLineComponentProps) {
  return (
    <g data-slot="flow-temporary-connection-line">
      <path
        fill="none"
        stroke="var(--muted-foreground)"
        strokeWidth={2}
        strokeDasharray="4 4"
        d={`M${fromX},${fromY} L${toX},${toY}`}
      />
      <circle
        cx={toX}
        cy={toY}
        r={18}
        fill="var(--muted)"
        stroke="var(--border)"
        strokeWidth={1}
        opacity={0.7}
      />
      <text
        x={toX}
        y={toY + 4}
        textAnchor="middle"
        fontSize={10}
        fill="var(--foreground)"
      >
        +
      </text>
    </g>
  );
}
