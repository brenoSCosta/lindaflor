import {
  useCallback,
  useLayoutEffect,
  useRef,
  type ReactNode,
  type Ref,
} from "react";

import { cn } from "@/lib/utils";

type RevealProps = {
  children: ReactNode;
  className?: string;
  delay?: number;
  threshold?: number;
  as?: "div" | "article" | "figure" | "section";
  ref?: Ref<HTMLElement>;
};

export function useRevealEnabled() {
  useLayoutEffect(() => {
    const media = window.matchMedia("(prefers-reduced-motion: reduce)");
    if (!media.matches) {
      document.documentElement.classList.add("reveal-enabled");
      return () => document.documentElement.classList.remove("reveal-enabled");
    }
    return undefined;
  }, []);
}

export function Reveal({
  children,
  className,
  delay = 0,
  threshold = 0,
  as: Component = "div",
  ref: externalRef,
}: RevealProps) {
  const ref = useRef<HTMLElement | null>(null);
  const setRef = useCallback(
    (node: HTMLElement | null) => {
      ref.current = node;
      if (externalRef) {
        if (typeof externalRef === "function") {
          externalRef(node);
        } else {
          externalRef.current = node;
        }
      }
    },
    [externalRef],
  );

  useLayoutEffect(() => {
    const node = ref.current;
    if (node) {
      node.style.transitionDelay = delay > 0 ? `${delay}ms` : "";

      const observer = new IntersectionObserver(
        (entries) => {
          if (!entries[0]?.isIntersecting) return;
          node.classList.add("reveal-in");
          observer.disconnect();
        },
        { rootMargin: "0px 0px -12% 0px", threshold },
      );

      observer.observe(node);
      return () => {
        observer.disconnect();
        node.style.transitionDelay = "";
      };
    }
    return undefined;
  }, [delay, threshold]);

  return (
    <Component ref={setRef} className={cn("reveal", className)}>
      {children}
    </Component>
  );
}
