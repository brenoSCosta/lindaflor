import { useEffect, type RefObject } from "react";

const ARC_CONE = 25;
const HALO_SOLID = 2.5;
const HALO_FADE = 10;

export function useBorderGlow(ref: RefObject<HTMLElement | null>) {
  useEffect(() => {
    const card = ref.current;
    if (!card) return undefined;

    if (window.matchMedia("(prefers-reduced-motion: reduce)").matches)
      return undefined;
    if (!window.matchMedia("(hover: hover) and (pointer: fine)").matches)
      return undefined;

    const arc = document.createElement("div");
    arc.className = "border-glow-arc";

    const halo = document.createElement("div");
    halo.className = "border-glow-halo";
    const haloGlow = document.createElement("div");
    haloGlow.className = "border-glow-halo-glow";
    halo.appendChild(haloGlow);

    card.prepend(halo, arc);

    const setMask = (el: HTMLElement, value: string) => {
      el.style.setProperty("-webkit-mask-image", value);
      el.style.setProperty("mask-image", value);
    };

    const onMove = (event: PointerEvent) => {
      const rect = card.getBoundingClientRect();
      const dx = event.clientX - rect.left - rect.width / 2;
      const dy = event.clientY - rect.top - rect.height / 2;

      const proximity = Math.min(
        1,
        Math.max(
          Math.abs(dx) / (rect.width / 2),
          Math.abs(dy) / (rect.height / 2),
        ),
      );
      let angle = (Math.atan2(dy, dx) * 180) / Math.PI + 90;
      if (angle < 0) angle += 360;

      arc.style.opacity = `${Math.max(0, (proximity - 0.5) / 0.5)}`;
      halo.style.opacity = `${Math.max(0, (proximity - 0.3) / 0.7)}`;

      setMask(
        arc,
        `conic-gradient(from ${angle}deg at center, #000 ${ARC_CONE}%, ` +
          `transparent ${ARC_CONE + 15}%, transparent ${100 - ARC_CONE - 15}%, ` +
          `#000 ${100 - ARC_CONE}%)`,
      );
      setMask(
        halo,
        `conic-gradient(from ${angle}deg at center, #000 ${HALO_SOLID}%, ` +
          `transparent ${HALO_FADE}%, transparent ${100 - HALO_FADE}%, ` +
          `#000 ${100 - HALO_SOLID}%)`,
      );
    };

    const onLeave = () => {
      arc.style.opacity = "0";
      halo.style.opacity = "0";
    };

    card.addEventListener("pointermove", onMove);
    card.addEventListener("pointerleave", onLeave);

    return () => {
      card.removeEventListener("pointermove", onMove);
      card.removeEventListener("pointerleave", onLeave);
      arc.remove();
      halo.remove();
    };
  }, [ref]);
}
