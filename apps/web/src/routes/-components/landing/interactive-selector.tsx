import { useCallback, useEffect, useRef, useState } from "react";

import { cn } from "@/lib/utils";
import type { SelectorOption } from "@/routes/-components/landing/content";

type InteractiveSelectorProps = {
  options: readonly SelectorOption[];
  className?: string;
  autoRotateMs?: number;
};

export function InteractiveSelector({
  options,
  className,
  autoRotateMs = 5000,
}: InteractiveSelectorProps) {
  const [activeIndex, setActiveIndex] = useState(0);
  const [animatedOptions, setAnimatedOptions] = useState<Set<number>>(
    new Set(),
  );
  const isPausedRef = useRef(false);
  const containerRef = useRef<HTMLDivElement | null>(null);
  const observerRef = useRef<IntersectionObserver | null>(null);
  const timersRef = useRef<number[]>([]);
  const intervalRef = useRef<number | null>(null);
  const animatedCountRef = useRef(0);

  const clearAnimationTimers = useCallback(() => {
    for (const timer of timersRef.current) {
      window.clearTimeout(timer);
    }
    timersRef.current = [];
  }, []);

  const stopAutoRotate = useCallback(() => {
    if (intervalRef.current) {
      window.clearInterval(intervalRef.current);
      intervalRef.current = null;
    }
  }, []);

  const startAutoRotate = useCallback(() => {
    if (
      intervalRef.current ||
      autoRotateMs <= 0 ||
      options.length === 0 ||
      isPausedRef.current ||
      animatedCountRef.current < options.length
    ) {
      return;
    }

    intervalRef.current = window.setInterval(() => {
      setActiveIndex((current) => (current + 1) % options.length);
    }, autoRotateMs);
  }, [autoRotateMs, options.length]);

  const animateOptions = useCallback(() => {
    clearAnimationTimers();
    stopAutoRotate();
    setAnimatedOptions(new Set());
    animatedCountRef.current = 0;

    const timers = options.map((_, index) =>
      window.setTimeout(() => {
        setAnimatedOptions((current) => {
          const next = current.has(index)
            ? current
            : new Set(current).add(index);
          animatedCountRef.current = next.size;

          if (next.size === options.length) {
            startAutoRotate();
          }

          return next;
        });
      }, 180 * index),
    );
    timersRef.current = timers;
  }, [clearAnimationTimers, options, startAutoRotate, stopAutoRotate]);

  const handleContainerRef = useCallback(
    (node: HTMLDivElement | null) => {
      if (observerRef.current) {
        observerRef.current.disconnect();
        observerRef.current = null;
      }

      containerRef.current = node;

      if (!node) {
        return;
      }

      if (typeof IntersectionObserver === "undefined") {
        animateOptions();
        return;
      }

      const observer = new IntersectionObserver(
        (entries) => {
          for (const entry of entries) {
            if (entry.isIntersecting) {
              animateOptions();
              observer.disconnect();
              break;
            }
          }
        },
        { threshold: 0.2 },
      );

      observer.observe(node);
      observerRef.current = observer;
    },
    [animateOptions],
  );

  useEffect(() => {
    return () => {
      clearAnimationTimers();
      stopAutoRotate();
    };
  }, [clearAnimationTimers, stopAutoRotate]);

  return (
    <div
      ref={handleContainerRef}
      className={cn(
        "relative flex w-full flex-col items-stretch overflow-hidden md:flex-row",
        className,
      )}
      onMouseEnter={() => {
        isPausedRef.current = true;
        stopAutoRotate();
      }}
      onMouseLeave={() => {
        isPausedRef.current = false;
        startAutoRotate();
      }}
      onFocusCapture={() => {
        isPausedRef.current = true;
        stopAutoRotate();
      }}
      onBlurCapture={(event) => {
        const nextFocused = event.relatedTarget;
        if (
          nextFocused instanceof Node &&
          event.currentTarget.contains(nextFocused)
        ) {
          return;
        }
        isPausedRef.current = false;
        startAutoRotate();
      }}
      role="presentation"
    >
      {options.map((option, index) => {
        const Icon = option.icon;
        const isActive = activeIndex === index;
        const isVisible = animatedOptions.has(index);
        const tags = option.tags ?? [];
        const revealClass = isActive
          ? "translate-x-0 opacity-100 delay-[600ms] duration-[600ms]"
          : "translate-x-[25px] opacity-0 delay-0 duration-0";

        return (
          <button
            key={option.title}
            type="button"
            className={cn(
              "relative flex min-w-15 flex-none cursor-pointer flex-col justify-end overflow-hidden border-2 bg-zinc-900 transition-all duration-700 ease-in-out will-change-[flex-grow,height,box-shadow] backface-hidden md:h-auto",
              isVisible
                ? "translate-x-0 opacity-100"
                : "-translate-x-16 opacity-0",
              isActive
                ? "z-10 h-125 border-secondary shadow-[0_20px_60px_rgba(0,0,0,0.50)] md:flex-7"
                : "z-1 h-22 border-background shadow-[0_10px_30px_rgba(0,0,0,0.30)] md:flex-1",
            )}
            aria-pressed={isActive}
            onClick={() => setActiveIndex(index)}
          >
            <img
              src={option.image}
              alt=""
              aria-hidden="true"
              loading="lazy"
              decoding="async"
              draggable={false}
              className={cn(
                "pointer-events-none absolute inset-0 size-full object-cover transition-transform duration-700 ease-in-out",
                isActive ? "scale-100" : "scale-[1.2]",
              )}
            />

            <div
              className={cn(
                "pointer-events-none absolute right-0 left-0 h-30 transition-all duration-700 ease-in-out",
                isActive
                  ? "bottom-0 shadow-[inset_0_-120px_120px_-120px_#000,inset_0_-120px_120px_-80px_#000]"
                  : "-bottom-10 shadow-[inset_0_-120px_0px_-120px_#000,inset_0_-120px_0px_-80px_#000]",
              )}
            />

            <div
              className={cn(
                "pointer-events-none absolute right-0 bottom-0 left-0 h-full bg-linear-to-t from-accent via-accent/85 to-accent/30 transition-opacity duration-700 ease-in-out md:h-1/2 md:via-accent/70 md:to-transparent",
                isActive ? "opacity-100" : "opacity-0",
              )}
            />

            <div className="pointer-events-none absolute right-0 bottom-0 left-0 z-2 flex w-full items-end justify-start gap-3 px-4 pb-5">
              <div className="relative flex h-11 max-w-11 min-w-11 shrink-0 grow-0 items-center justify-center rounded-full border-2 border-border bg-zinc-900/85 shadow-[0_1px_4px_rgba(0,0,0,0.18)] backdrop-blur-[10px] transition-all duration-200">
                <Icon
                  size={24}
                  className="text-background dark:text-foreground"
                />
              </div>
              <div className="relative flex w-full flex-col text-background md:flex-row dark:text-foreground">
                <div>
                  <div
                    className={cn(
                      "text-lg font-bold transition-all ease-in-out",
                      revealClass,
                    )}
                  >
                    {option.title}
                  </div>
                  <div
                    className={cn(
                      "text-base text-background transition-all ease-in-out dark:text-foreground",
                      revealClass,
                    )}
                  >
                    {option.description}
                  </div>
                </div>
                {tags.length > 0 && (
                  <ul
                    className={cn(
                      "mt-2.5 flex flex-wrap justify-start gap-1.5 transition-all ease-in-out md:justify-end",
                      revealClass,
                    )}
                  >
                    {tags.map((tag) => (
                      <li
                        key={tag}
                        className="border border-background/30 px-2 py-0.5 font-mono text-[0.625rem] tracking-wide text-background uppercase dark:border-foreground/30 dark:text-foreground"
                      >
                        {tag}
                      </li>
                    ))}
                  </ul>
                )}
              </div>
            </div>
          </button>
        );
      })}
    </div>
  );
}
