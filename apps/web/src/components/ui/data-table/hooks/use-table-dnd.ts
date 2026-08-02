import {
  KeyboardSensor,
  PointerSensor,
  closestCenter,
  pointerWithin,
  useSensor,
  useSensors,
  type CollisionDetection,
  type DragEndEvent,
} from "@dnd-kit/core";
import { arrayMove, sortableKeyboardCoordinates } from "@dnd-kit/sortable";
import type { RowData } from "@tanstack/react-table";

import { GROUP_DROPZONE_ID } from "@/components/ui/data-table/components/toolbar/data-table-grouping";
import type { DataTableInstance } from "@/components/ui/data-table/core/types";

// The group drop zone should accept a drop anywhere inside its bounds, not
// only near its center. Prefer a pointer-within hit on the zone; otherwise
// fall back to closestCenter for column/row reordering (and keyboard dnd,
// where pointerWithin yields nothing).
const collisionDetection: CollisionDetection = (args) => {
  const groupHit = pointerWithin(args).find((c) => c.id === GROUP_DROPZONE_ID);
  return groupHit ? [groupHit] : closestCenter(args);
};

/**
 * Sensors + drag-end handler for the single `DndContext` that drives both
 * column and row reordering. The active item's `data.type` (set in useSortable)
 * routes to the right handler — two nested contexts can't be used: dnd-kit
 * renders an a11y `<div>` that is invalid inside `<tbody>`, and a body-wrapping
 * context would swallow header drags.
 */
export function useTableDnd<TData extends RowData>(
  table: DataTableInstance<TData>,
) {
  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 5 } }),
    useSensor(KeyboardSensor, {
      coordinateGetter: sortableKeyboardCoordinates,
    }),
  );

  const onRowOrderChange = table.tableInstance.onRowOrderChange;

  const handleDragEnd = (event: DragEndEvent) => {
    const { active, over } = event;
    if (!over) return;
    const type = active.data.current?.type;

    if (type === "row") {
      if (active.id !== over.id) {
        onRowOrderChange?.(String(active.id), String(over.id));
      }
      return;
    }

    // column drag → drop on the group zone groups by that column
    if (over.id === GROUP_DROPZONE_ID) {
      const column = table.getColumn(String(active.id));
      if (column && !column.getIsGrouped()) column.toggleGrouping();
      return;
    }
    // Otherwise it's a reorder, which only applies when ordering is enabled
    // (a column may be draggable solely to support drag-to-group).
    if (!table.tableInstance.enableColumnOrdering) return;
    if (active.id === over.id) return;
    const base =
      table.getState().columnOrder.length > 0
        ? table.getState().columnOrder
        : table.getAllLeafColumns().map((c) => c.id);
    const oldIndex = base.indexOf(String(active.id));
    const newIndex = base.indexOf(String(over.id));
    if (oldIndex === -1 || newIndex === -1) return;
    table.setColumnOrder(arrayMove(base, oldIndex, newIndex));
  };

  return { sensors, collisionDetection, handleDragEnd };
}
