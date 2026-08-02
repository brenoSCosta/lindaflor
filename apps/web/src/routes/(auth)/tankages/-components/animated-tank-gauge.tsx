import {
  animate,
  domAnimation,
  LazyMotion,
  m,
  useMotionValue,
  useReducedMotion,
  useTransform,
} from "motion/react";
import { useEffect } from "react";

import { cn } from "@/lib/utils";

/** Fill-level tone: room to spare → approaching capacity → near/full. */
export type TankFillTone = "primary" | "warning" | "destructive";

/** Shared timing so the liquid rise and the counting number stay in sync. */
const FILL_DURATION = 1.2;
const FILL_EASE: [number, number, number, number] = [0.22, 1, 0.36, 1];

const toneClassName: Record<TankFillTone, { liquid: string; wave: string }> = {
  primary: { liquid: "bg-primary", wave: "text-primary" },
  warning: { liquid: "bg-warning", wave: "text-warning" },
  destructive: { liquid: "bg-destructive", wave: "text-destructive" },
};

const sizeClassName = {
  sm: "h-32 w-14",
  md: "h-44 w-24",
} as const;

/**
 * Two identical wave segments (each 120 units wide) so translating the SVG by
 * exactly one container width loops seamlessly.
 */
const WAVE_PATH =
  "M0,8 C20,0 40,0 60,8 C80,16 100,16 120,8 C140,0 160,0 180,8 C200,16 220,16 240,8 L240,24 L0,24 Z";

type AnimatedTankGaugeProps = {
  fillPercent: number | null;
  tone: TankFillTone;
  size?: keyof typeof sizeClassName;
  className?: string;
};

/**
 * Stylised cylindrical tank whose liquid rises to the fill percentage on mount,
 * with a rippling surface. Falls back to a static level when the user prefers
 * reduced motion or when the capacity is unknown.
 */
export function AnimatedTankGauge({
  fillPercent,
  tone,
  size = "sm",
  className,
}: AnimatedTankGaugeProps) {
  const reduceMotion = useReducedMotion();
  const known = fillPercent != null;
  const clamped = known ? Math.min(100, Math.max(0, fillPercent)) : 0;
  const colors = toneClassName[tone];
  const waveHeight = size === "md" ? 16 : 11;

  return (
    <LazyMotion features={domAnimation}>
      <div
        role="progressbar"
        aria-valuenow={fillPercent ?? undefined}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-label={
          known ? `${clamped}% da capacidade` : "Capacidade desconhecida"
        }
        className={cn(
          "relative shrink-0 overflow-hidden rounded-xl border bg-muted/50 ring-1 ring-black/5 ring-inset",
          sizeClassName[size],
          className,
        )}
      >
        <m.div
          className="absolute inset-x-0 bottom-0"
          style={reduceMotion ? { height: `${clamped}%` } : undefined}
          initial={reduceMotion ? false : { height: "0%" }}
          animate={reduceMotion ? undefined : { height: `${clamped}%` }}
          transition={{ duration: FILL_DURATION, ease: FILL_EASE }}
        >
          <div className={cn("absolute inset-0", colors.liquid)} />
          <div className="absolute inset-0 bg-linear-to-b from-white/25 via-transparent to-black/10" />
          <div
            className={cn(
              "absolute inset-x-0 top-0 -translate-y-[55%]",
              colors.wave,
            )}
            style={{ height: waveHeight }}
          >
            <m.svg
              viewBox="0 0 240 24"
              preserveAspectRatio="none"
              aria-hidden
              className="absolute top-0 left-0 h-full w-[200%] fill-current"
              initial={false}
              animate={reduceMotion ? undefined : { x: ["0%", "-50%"] }}
              transition={
                reduceMotion
                  ? undefined
                  : { duration: 2.6, ease: "linear", repeat: Infinity }
              }
            >
              <path d={WAVE_PATH} />
            </m.svg>
          </div>
        </m.div>

        <div
          className="pointer-events-none absolute inset-0 flex flex-col justify-between py-2 opacity-20"
          aria-hidden
        >
          <div className="border-t border-foreground" />
          <div className="w-2/3 border-t border-foreground" />
          <div className="border-t border-foreground" />
          <div className="w-2/3 border-t border-foreground" />
          <div className="border-t border-foreground" />
        </div>

        {!known ? (
          <div className="absolute inset-0 flex items-center justify-center">
            <span className="font-mono text-lg text-muted-foreground">—</span>
          </div>
        ) : null}
      </div>
    </LazyMotion>
  );
}

type AnimatedTankPercentProps = {
  value: number | null;
  className?: string;
  suffixClassName?: string;
};

/**
 * Counts from 0 up to the fill percentage using the same timing as
 * {@link AnimatedTankGauge}, so the number and the liquid settle together.
 */
export function AnimatedTankPercent({
  value,
  className,
  suffixClassName,
}: AnimatedTankPercentProps) {
  const reduceMotion = useReducedMotion();
  const count = useMotionValue(0);
  const rounded = useTransform(count, (latest) => Math.round(latest));

  useEffect(() => {
    if (value == null) return undefined;
    if (reduceMotion) {
      count.set(value);
      return undefined;
    }
    const controls = animate(count, value, {
      duration: FILL_DURATION,
      ease: FILL_EASE,
    });
    return () => controls.stop();
  }, [value, reduceMotion, count]);

  if (value == null) {
    return <span className={className}>—</span>;
  }

  return (
    <LazyMotion features={domAnimation}>
      <span className={className}>
        <m.span>{rounded}</m.span>
        <span className={suffixClassName}>%</span>
      </span>
    </LazyMotion>
  );
}
