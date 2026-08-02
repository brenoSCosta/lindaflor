import { isElementTarget } from "@/components/ui/flow/helpers/is-element-target";

export function paneRelativePointer(
  event: { clientX: number; clientY: number; target: EventTarget | null },
  paneSelector = ".react-flow",
): { top: number; left: number } | null {
  if (!isElementTarget(event.target)) return null;
  const pane = event.target.closest(paneSelector);
  const bounds = pane?.getBoundingClientRect();
  if (!bounds) return null;
  return {
    top: event.clientY - bounds.top,
    left: event.clientX - bounds.left,
  };
}
