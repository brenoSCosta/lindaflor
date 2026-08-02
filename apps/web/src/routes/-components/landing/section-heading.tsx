import { cn } from "@/lib/utils";
import { Reveal } from "@/routes/-components/landing/reveal";

type SectionHeadingProps = {
  eyebrow: string;
  title: string;
  lead?: string;
  align?: "left" | "center";
  invert?: boolean;
};

export function SectionHeading({
  eyebrow,
  title,
  lead,
  align = "left",
  invert = false,
}: SectionHeadingProps) {
  const centered = align === "center";

  return (
    <Reveal
      className={cn(
        "flex flex-col",
        centered ? "items-center text-center" : "items-start",
      )}
    >
      <div className="flex items-center gap-3 font-mono text-xs tracking-[0.22em] uppercase">
        {centered && (
          <span
            className={cn(
              "h-px w-8",
              invert ? "bg-primary-foreground/25" : "bg-border",
            )}
          />
        )}
        <span
          className={cn(
            invert ? "text-primary-foreground/65" : "text-muted-foreground",
          )}
        >
          {eyebrow}
        </span>
        {centered && (
          <span
            className={cn(
              "h-px w-8",
              invert ? "bg-primary-foreground/25" : "bg-border",
            )}
          />
        )}
      </div>

      <h2
        className={cn(
          "mt-5 max-w-3xl font-display text-3xl font-bold tracking-tight text-balance md:text-4xl",
          invert ? "text-primary-foreground" : "text-foreground",
        )}
      >
        {title}
      </h2>

      {centered && <span className="mt-5 h-1 w-14 bg-secondary" />}

      {lead && (
        <p
          className={cn(
            "mt-4 max-w-2xl leading-relaxed whitespace-pre-line",
            invert ? "text-primary-foreground/75" : "text-muted-foreground",
          )}
        >
          {lead}
        </p>
      )}
    </Reveal>
  );
}
