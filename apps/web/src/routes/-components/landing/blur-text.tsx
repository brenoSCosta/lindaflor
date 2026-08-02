import { useId, useMemo } from "react";

import { cn, cssVar } from "@/lib/utils";

type BlurTextProps = {
  words: readonly string[];
  className?: string;
  startDelay?: number;
  delay?: number;
};

export function BlurText({
  words,
  className,
  startDelay = 0,
  delay = 90,
}: BlurTextProps) {
  const baseId = useId();
  const text = words.join(" ");

  const wordEntries = useMemo(
    () => words.map((word, index) => ({ id: `${baseId}-w-${index}`, word })),
    [words, baseId],
  );

  return (
    <span className={className} aria-label={text}>
      <span className="sr-only">{text}</span>
      {wordEntries.map(({ id, word }, index) => (
        <span
          key={id}
          aria-hidden="true"
          className={cn("blur-text-word hero-fade-in")}
          style={cssVar("--fade-delay", `${startDelay + index * delay}ms`)}
        >
          {word}
        </span>
      ))}
    </span>
  );
}
