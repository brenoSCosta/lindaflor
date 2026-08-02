import { useEffect, type RefObject } from "react";

interface BentoOptions {
  particleCount?: number;
  spotlightRadius?: number;
}

const SPAWN_EASE = "cubic-bezier(0.34, 1.56, 0.64, 1)";

export function useBento(
  ref: RefObject<HTMLElement | null>,
  options?: BentoOptions,
) {
  useEffect(() => {
    const grid = ref.current;
    if (!grid) return () => undefined;

    const particleCount = options?.particleCount ?? 12;
    const spotlightRadius = options?.spotlightRadius ?? 320;

    const cards = Array.from(grid.querySelectorAll<HTMLElement>(".bento-card"));

    let entrance: IntersectionObserver | null = null;
    let spotlightLayer: HTMLElement | null = null;
    let gridOnMove: ((event: MouseEvent) => void) | null = null;
    let gridOnLeave: (() => void) | null = null;

    const reducedMotion = window.matchMedia(
      "(prefers-reduced-motion: reduce)",
    ).matches;

    if (reducedMotion) {
      cards.forEach((card) => card.classList.add("bento-in"));
      return () => undefined;
    }

    // ── Staggered scroll-in entrance ──────────────────────────────────────
    const cardIndex = new Map<HTMLElement, number>();
    cards.forEach((card, i) => cardIndex.set(card, i));

    entrance = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (!entry.isIntersecting) continue;
          if (!(entry.target instanceof HTMLElement)) continue;
          const card = entry.target;
          const index = cardIndex.get(card) ?? 0;
          window.setTimeout(() => card.classList.add("bento-in"), index * 80);
          entrance?.unobserve(card);
        }
      },
      { threshold: 0, rootMargin: "0px 0px -12% 0px" },
    );
    cards.forEach((card) => entrance?.observe(card));

    // Cursor effects only for fine pointers with hover
    const finePointer = window.matchMedia(
      "(hover: hover) and (pointer: fine)",
    ).matches;

    if (!finePointer) {
      return () => {
        entrance?.disconnect();
      };
    }

    // ── Global spotlight ──────────────────────────────────────────────────
    spotlightLayer = document.createElement("div");
    spotlightLayer.className = "bento-spotlight-layer";
    const spotlight = document.createElement("div");
    spotlight.className = "bento-spotlight";
    spotlightLayer.appendChild(spotlight);
    grid.prepend(spotlightLayer);

    const proximity = spotlightRadius * 0.5;
    const fadeDistance = spotlightRadius * 0.75;

    const cardListeners: Array<{
      card: HTMLElement;
      onEnter: () => void;
      onLeave: () => void;
      onMove: (event: MouseEvent) => void;
      onClick: (event: MouseEvent) => void;
    }> = [];

    gridOnMove = (event: MouseEvent) => {
      const gridRect = grid.getBoundingClientRect();
      spotlight.style.left = `${event.clientX - gridRect.left}px`;
      spotlight.style.top = `${event.clientY - gridRect.top}px`;

      let minDistance = Infinity;
      for (const card of cards) {
        const rect = card.getBoundingClientRect();
        const centerX = rect.left + rect.width / 2;
        const centerY = rect.top + rect.height / 2;
        const edgeDistance = Math.max(
          0,
          Math.hypot(event.clientX - centerX, event.clientY - centerY) -
            Math.max(rect.width, rect.height) / 2,
        );
        minDistance = Math.min(minDistance, edgeDistance);

        let intensity = 0;
        if (edgeDistance <= proximity) intensity = 1;
        else if (edgeDistance <= fadeDistance)
          intensity =
            (fadeDistance - edgeDistance) / (fadeDistance - proximity);

        card.style.setProperty(
          "--glow-x",
          `${((event.clientX - rect.left) / rect.width) * 100}%`,
        );
        card.style.setProperty(
          "--glow-y",
          `${((event.clientY - rect.top) / rect.height) * 100}%`,
        );
        card.style.setProperty("--glow-intensity", `${intensity}`);
        card.style.setProperty("--glow-radius", `${spotlightRadius}px`);
      }

      spotlight.style.opacity = `${
        minDistance <= proximity
          ? 0.8
          : minDistance <= fadeDistance
            ? ((fadeDistance - minDistance) / (fadeDistance - proximity)) * 0.8
            : 0.25
      }`;
    };

    gridOnLeave = () => {
      spotlight.style.opacity = "0";
      for (const card of cards) card.style.setProperty("--glow-intensity", "0");
    };

    grid.addEventListener("mousemove", gridOnMove);
    grid.addEventListener("mouseleave", gridOnLeave);

    // ── Per-card effects: particles, tilt, magnetism, click ripple ────────
    for (const card of cards) {
      let hovered = false;
      const spawnTimers: number[] = [];
      const particles: HTMLElement[] = [];

      const clearParticles = () => {
        spawnTimers.forEach((timer) => window.clearTimeout(timer));
        spawnTimers.length = 0;
        particles.forEach((particle) => {
          particle.getAnimations().forEach((animation) => animation.cancel());
          const exit = particle.animate(
            [
              { transform: "scale(1)", opacity: 1 },
              { transform: "scale(0)", opacity: 0 },
            ],
            { duration: 300, easing: "ease-in", fill: "forwards" },
          );
          exit.onfinish = () => particle.remove();
        });
        particles.length = 0;
      };

      const spawnParticle = () => {
        if (!hovered) return;
        const { width, height } = card.getBoundingClientRect();
        const particle = document.createElement("div");
        particle.className = "bento-particle";
        particle.style.left = `${Math.random() * width}px`;
        particle.style.top = `${Math.random() * height}px`;
        card.appendChild(particle);
        particles.push(particle);

        particle.animate(
          [{ transform: "scale(0)" }, { transform: "scale(1)" }],
          {
            duration: 300,
            easing: SPAWN_EASE,
            fill: "forwards",
          },
        );
        particle.animate(
          [
            { transform: "translate(0px, 0px) rotate(0deg)" },
            {
              transform: `translate(${(Math.random() - 0.5) * 100}px, ${(Math.random() - 0.5) * 100}px) rotate(${Math.random() * 360}deg)`,
            },
          ],
          {
            duration: 2000 + Math.random() * 2000,
            easing: "ease-in-out",
            iterations: Infinity,
            direction: "alternate",
            composite: "add",
          },
        );
        particle.animate([{ opacity: 1 }, { opacity: 0.3 }], {
          duration: 1500,
          easing: "ease-in-out",
          iterations: Infinity,
          direction: "alternate",
        });
      };

      const onEnter = () => {
        hovered = true;
        for (let i = 0; i < particleCount; i++) {
          spawnTimers.push(window.setTimeout(spawnParticle, i * 100));
        }
      };

      const onLeave = () => {
        hovered = false;
        clearParticles();
        card.style.setProperty("--tilt-x", "0deg");
        card.style.setProperty("--tilt-y", "0deg");
        card.style.setProperty("--mag-x", "0px");
        card.style.setProperty("--mag-y", "0px");
      };

      const onMove = (event: MouseEvent) => {
        const rect = card.getBoundingClientRect();
        const offsetX = event.clientX - rect.left - rect.width / 2;
        const offsetY = event.clientY - rect.top - rect.height / 2;
        card.style.setProperty(
          "--tilt-x",
          `${(offsetY / rect.height) * -12}deg`,
        );
        card.style.setProperty("--tilt-y", `${(offsetX / rect.width) * 12}deg`);
        card.style.setProperty("--mag-x", `${offsetX * 0.04}px`);
        card.style.setProperty("--mag-y", `${offsetY * 0.04}px`);
      };

      const onClick = (event: MouseEvent) => {
        const rect = card.getBoundingClientRect();
        const x = event.clientX - rect.left;
        const y = event.clientY - rect.top;
        const radius = Math.max(
          Math.hypot(x, y),
          Math.hypot(x - rect.width, y),
          Math.hypot(x, y - rect.height),
          Math.hypot(x - rect.width, y - rect.height),
        );
        const ripple = document.createElement("div");
        ripple.className = "bento-ripple";
        ripple.style.width = `${radius * 2}px`;
        ripple.style.height = `${radius * 2}px`;
        ripple.style.left = `${x - radius}px`;
        ripple.style.top = `${y - radius}px`;
        card.appendChild(ripple);
        const animation = ripple.animate(
          [
            { transform: "scale(0)", opacity: 1 },
            { transform: "scale(1)", opacity: 0 },
          ],
          { duration: 800, easing: "ease-out" },
        );
        animation.onfinish = () => ripple.remove();
      };

      card.addEventListener("mouseenter", onEnter);
      card.addEventListener("mouseleave", onLeave);
      card.addEventListener("mousemove", onMove);
      card.addEventListener("click", onClick);

      cardListeners.push({ card, onEnter, onLeave, onMove, onClick });
    }

    return () => {
      entrance?.disconnect();
      spotlightLayer?.remove();
      if (gridOnMove) grid.removeEventListener("mousemove", gridOnMove);
      if (gridOnLeave) grid.removeEventListener("mouseleave", gridOnLeave);
      for (const { card, onEnter, onLeave, onMove, onClick } of cardListeners) {
        card.removeEventListener("mouseenter", onEnter);
        card.removeEventListener("mouseleave", onLeave);
        card.removeEventListener("mousemove", onMove);
        card.removeEventListener("click", onClick);
      }
    };
  }, [ref, options?.particleCount, options?.spotlightRadius]);
}
