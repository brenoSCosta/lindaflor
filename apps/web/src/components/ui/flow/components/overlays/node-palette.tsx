import type { DragEvent } from "react";

import { onFlowDragStart } from "@/components/ui/flow/helpers/dnd-transfer";

export type NodePaletteItem = { type: string; label: string };

export type NodePaletteProps = {
  items: NodePaletteItem[];
};

export function NodePalette({ items }: NodePaletteProps) {
  return (
    <aside
      data-slot="flow-node-palette"
      className="border-border bg-background w-28 shrink-0 space-y-2 border-r p-2 text-xs"
    >
      {items.map((item) => (
        <div
          key={item.type}
          className="cursor-grab rounded border p-2"
          draggable
          onDragStart={(e: DragEvent) => onFlowDragStart(e, item.type)}
        >
          {item.label}
        </div>
      ))}
    </aside>
  );
}
