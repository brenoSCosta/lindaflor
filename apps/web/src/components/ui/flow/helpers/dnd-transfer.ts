import type { DragEvent } from "react";

export const FLOW_DND_MIME = "application/reactflow";

export function onFlowDragOver(event: DragEvent) {
  event.preventDefault();
  event.dataTransfer.dropEffect = "move";
}

export function onFlowDragStart(event: DragEvent, type: string) {
  event.dataTransfer.setData(FLOW_DND_MIME, type);
  event.dataTransfer.effectAllowed = "move";
}

export function getFlowDragType(event: DragEvent, fallback = "default") {
  return event.dataTransfer.getData(FLOW_DND_MIME) || fallback;
}
