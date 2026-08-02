import { formatInTimeZone } from "date-fns-tz";
import { ptBR } from "date-fns/locale";

import { useTimeFormat } from "@/context/time-format";
import type { TimeFormatStr } from "@/context/time-format-options";
import { useTimezone } from "@/context/timezone";
import { cn } from "@/lib/utils";

interface TimeProps {
  date: Date | null | undefined;
  /**
   * Overrides the user-selected global format. When omitted, the value from
   * {@link useTimeFormat} is used so every `<Time>` instance renders with the
   * same preference chosen in the header switcher.
   */
  formatStr?: TimeFormatStr;
}

export function Time({
  className,
  date,
  formatStr: formatStrProp,
  ...props
}: React.ComponentProps<"time"> & TimeProps) {
  const { timezone } = useTimezone();
  const { formatStr: selectedFormatStr } = useTimeFormat();
  const formatStr = formatStrProp ?? selectedFormatStr;
  if (date == null || Number.isNaN(date.getTime())) return null;
  const formatted = formatInTimeZone(date, timezone, formatStr, {
    // TODO: LOCALE IN CONTEXT PROVIDER
    locale: ptBR,
  });

  const utc = date.toISOString();
  return (
    <time
      dateTime={utc}
      title={`${utc} (${timezone})`}
      className={cn("text-muted-foreground", className)}
      {...props}
    >
      {formatted}
    </time>
  );
}
