export type FlowRect = { x: number; y: number; w: number; h: number };

export type SelectionRectProps = {
  rect: FlowRect;
  variant?: "lasso" | "draw";
};

export function SelectionRect({ rect, variant = "lasso" }: SelectionRectProps) {
  return (
    <div
      data-slot="flow-selection-rect"
      className={
        variant === "lasso"
          ? "pointer-events-none absolute border border-dashed border-primary bg-primary/10"
          : "pointer-events-none absolute border-2 border-primary bg-primary/5"
      }
      style={{
        left: rect.x,
        top: rect.y,
        width: rect.w,
        height: rect.h,
      }}
    />
  );
}
