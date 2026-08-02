import {
  useCallback,
  useEffect,
  useId,
  useRef,
  useState,
  type ReactNode,
} from "react";

type MarqueeProps<T> = {
  items: readonly T[];
  renderItem: (item: T, index: number) => ReactNode;
  speed?: number;
  direction?: "left" | "right";
  gap?: number;
  pauseOnHover?: boolean;
  hoverSpeed?: number;
};

const SMOOTH_TAU = 0.25;
const MIN_COPIES = 2;
const COPY_HEADROOM = 2;

function Slot<T>({
  item,
  render: r,
  index: i,
}: {
  item: T;
  render: (item: T, index: number) => ReactNode;
  index: number;
}) {
  return r(item, i);
}

export function Marquee<T>({
  items,
  renderItem,
  speed = 40,
  direction = "left",
  gap = 56,
  pauseOnHover = true,
  hoverSpeed,
}: MarqueeProps<T>) {
  const baseId = useId();
  const trackRef = useRef<HTMLDivElement>(null);
  const seqWidthRef = useRef(0);
  const observerRef = useRef<ResizeObserver | null>(null);
  const [copyCount, setCopyCount] = useState(MIN_COPIES);
  const hoveredRef = useRef(false);

  const dirSign = direction === "left" ? 1 : -1;
  const targetVelocity = Math.abs(speed) * dirSign;
  const hoverVelocity =
    hoverSpeed !== undefined
      ? Math.abs(hoverSpeed) * dirSign
      : pauseOnHover
        ? 0
        : undefined;

  const containerRef = useCallback(
    (node: HTMLDivElement | null) => {
      void items;
      void gap;

      if (observerRef.current) {
        observerRef.current.disconnect();
        observerRef.current = null;
      }

      if (!node) return;

      const measure = () => {
        const seqEl = node.querySelector("ul");
        if (!seqEl) return;
        const width = Math.ceil(seqEl.getBoundingClientRect().width);
        if (width <= 0) return;
        seqWidthRef.current = width;
        setCopyCount(
          Math.max(
            MIN_COPIES,
            Math.ceil(node.clientWidth / width) + COPY_HEADROOM,
          ),
        );
      };

      measure();

      const observer = new ResizeObserver(measure);
      observerRef.current = observer;
      observer.observe(node);
      const seqEl = node.querySelector("ul");
      if (seqEl) observer.observe(seqEl);
    },
    [items, gap],
  );

  useEffect(() => {
    const track = trackRef.current;
    const seqWidth = seqWidthRef.current;
    if (!track || seqWidth <= 0) return undefined;

    if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
      track.style.transform = "translate3d(0, 0, 0)";
      return undefined;
    }

    let raf = 0;
    let last: number | null = null;
    let offset = 0;
    let velocity = 0;

    const step = (now: number) => {
      if (last === null) last = now;
      const dt = Math.max(0, now - last) / 1000;
      last = now;

      const target =
        hoveredRef.current && hoverVelocity !== undefined
          ? hoverVelocity
          : targetVelocity;
      velocity += (target - velocity) * (1 - Math.exp(-dt / SMOOTH_TAU));

      offset =
        (((offset + velocity * dt) % seqWidthRef.current) +
          seqWidthRef.current) %
        seqWidthRef.current;
      track.style.transform = `translate3d(${-offset}px, 0, 0)`;
      raf = requestAnimationFrame(step);
    };

    raf = requestAnimationFrame(step);
    return () => cancelAnimationFrame(raf);
  }, [copyCount, targetVelocity, hoverVelocity]);

  return (
    <div ref={containerRef} className="relative overflow-hidden">
      <div
        ref={trackRef}
        className="flex w-max will-change-transform"
        role="presentation"
        onPointerEnter={() => {
          hoveredRef.current = true;
        }}
        onPointerLeave={() => {
          hoveredRef.current = false;
        }}
      >
        {Array.from({ length: copyCount }, (_, copy) => (
          <ul
            key={copy}
            className="flex shrink-0 items-center"
            style={{ gap: `${gap}px`, paddingRight: `${gap}px` }}
            aria-hidden={copy > 0}
          >
            {(() => {
              const slots: ReactNode[] = [];
              for (let j = 0; j < items.length; j++) {
                slots.push(
                  <li key={`${baseId}-${copy}-${j}`} className="flex-none">
                    <Slot item={items[j]} render={renderItem} index={j} />
                  </li>,
                );
              }
              return slots;
            })()}
          </ul>
        ))}
      </div>
    </div>
  );
}
