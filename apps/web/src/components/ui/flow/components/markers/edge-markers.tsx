export function EdgeMarkerDefs() {
  return (
    <svg
      data-slot="flow-edge-marker-defs"
      aria-hidden
      className="absolute size-0"
    >
      <defs>
        <marker
          id="flow-custom-marker"
          markerWidth="12"
          markerHeight="12"
          viewBox="-10 -10 20 20"
          orient="auto"
          refX="0"
          refY="0"
        >
          <polyline
            stroke="var(--foreground)"
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth="1"
            fill="none"
            points="-5,-4 0,0 -5,4"
          />
        </marker>
      </defs>
    </svg>
  );
}

export const FLOW_CUSTOM_MARKER_ID = "flow-custom-marker";
