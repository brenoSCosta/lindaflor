import type { ReactNode } from "react";

export type NodeContextMenuState = {
  id: string;
  top: number;
  left: number;
} | null;

export type NodeContextMenuProps = {
  menu: NodeContextMenuState;
  children: ReactNode;
};

export function NodeContextMenu({ menu, children }: NodeContextMenuProps) {
  if (!menu) return null;
  return (
    <div
      data-slot="flow-node-context-menu"
      className="bg-popover text-popover-foreground absolute z-50 rounded border p-1 shadow-md"
      style={{ top: menu.top, left: menu.left }}
    >
      {children}
    </div>
  );
}
