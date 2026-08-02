import {
  useRef,
  useState,
  type PointerEvent as ReactPointerEvent,
  type RefObject,
} from "react";

import type { FlowRect } from "@/components/ui/flow/components/overlays/selection-rect";
import { isElementTarget } from "@/components/ui/flow/helpers/is-element-target";

export function usePaneRectGesture(wrapper: RefObject<HTMLElement | null>) {
  const [rect, setRect] = useState<FlowRect | null>(null);
  const start = useRef<{ x: number; y: number } | null>(null);
  const rectRef = useRef<FlowRect | null>(null);

  function onPointerDown(event: ReactPointerEvent) {
    if (
      !isElementTarget(event.target) ||
      !event.target.classList.contains("react-flow__pane")
    ) {
      return;
    }
    const bounds = wrapper.current?.getBoundingClientRect();
    if (!bounds) return;
    start.current = {
      x: event.clientX - bounds.left,
      y: event.clientY - bounds.top,
    };
    const next = { x: start.current.x, y: start.current.y, w: 0, h: 0 };
    rectRef.current = next;
    setRect(next);
  }

  function onPointerMove(event: ReactPointerEvent) {
    if (!start.current || !wrapper.current) return;
    const bounds = wrapper.current.getBoundingClientRect();
    const x = event.clientX - bounds.left;
    const y = event.clientY - bounds.top;
    const next = {
      x: Math.min(start.current.x, x),
      y: Math.min(start.current.y, y),
      w: Math.abs(x - start.current.x),
      h: Math.abs(y - start.current.y),
    };
    rectRef.current = next;
    setRect(next);
  }

  function endGesture() {
    const finished = rectRef.current;
    start.current = null;
    rectRef.current = null;
    setRect(null);
    return finished;
  }

  return { rect, onPointerDown, onPointerMove, endGesture, setRect };
}
