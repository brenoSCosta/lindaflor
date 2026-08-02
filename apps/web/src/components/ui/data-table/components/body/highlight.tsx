import React from "react";

import { cn } from "@/lib/utils";

const MAX_HIGHLIGHT_LENGTH = 2000;

function escapeRegExp(input: string): string {
  return input.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

/**
 * Wraps occurrences of `query` within `text` in a themeable `<mark>`. Work is
 * skipped when there's no query, the query is escaped, and very long strings
 * are left unhighlighted to avoid regex blowups on huge cells.
 */
export const Highlight = React.memo(function Highlight({
  text,
  query,
  className,
}: {
  text: string;
  query?: string | null;
  className?: string;
}) {
  const trimmed = query?.trim();
  if (!trimmed || text.length === 0 || text.length > MAX_HIGHLIGHT_LENGTH) {
    return text;
  }

  const parts = text.split(new RegExp(`(${escapeRegExp(trimmed)})`, "gi"));
  if (parts.length === 1) return text;

  const needle = trimmed.toLowerCase();

  const partObjects = parts.map((part, index) => ({
    id: `part-${part.slice(0, 5)}-${index}`,
    text: part,
  }));

  return (
    <>
      {partObjects.map((item) => {
        const isMatch = item.text.toLowerCase() === needle;

        return isMatch ? (
          <mark
            key={item.id}
            className={cn(
              "rounded-xs bg-highlight px-0.5 text-highlight-foreground",
              className,
            )}
          >
            {item.text}
          </mark>
        ) : (
          <React.Fragment key={item.id}>{item.text}</React.Fragment>
        );
      })}
    </>
  );
});
