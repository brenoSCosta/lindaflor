import {
  calendarDateToDayKey,
  dayKeyToCalendarDate,
  zonedDateTimeToUtc,
  zonedParts,
} from "@lindaflor/shared/lib/zoned-datetime";
import type { TankageTimeWindow } from "@lindaflor/shared/schemas/tankage/tankages";
import { ChevronDownIcon } from "lucide-react";
import React from "react";

import { Button } from "@/components/ui/button";
import { Calendar } from "@/components/ui/calendar";
import { Field, FieldLabel } from "@/components/ui/field";
import {
  NativeSelect,
  NativeSelectOption,
} from "@/components/ui/native-select";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
import { Time } from "@/components/ui/time";
import { useTimezone } from "@/context/timezone";

const ALL_HOURS = Array.from({ length: 24 }, (_, index) =>
  String(index).padStart(2, "0"),
);
const ALL_MINUTES = Array.from({ length: 60 }, (_, index) =>
  String(index).padStart(2, "0"),
);

interface MeasuredAtFieldProps {
  id?: string;
  value: Date;
  onChange: (date: Date) => void;
  fixedDay?: string;
  /** When set, hour/minute options are limited to the open interval between neighbors. */
  timeWindow?: TankageTimeWindow;
}

function minutesOfDay(date: Date, timezone: string): number {
  const parts = zonedParts(date, timezone);
  return Number(parts.hour) * 60 + Number(parts.minute);
}

function allowedHoursMinutes(args: {
  dayKey: string;
  timezone: string;
  timeWindow?: TankageTimeWindow;
}): { hours: string[]; minutesForHour: (hour: string) => string[] } {
  const { dayKey, timezone, timeWindow } = args;
  if (timeWindow == null) {
    return {
      hours: ALL_HOURS,
      minutesForHour: () => ALL_MINUTES,
    };
  }

  const dayStart = zonedDateTimeToUtc(dayKey, "00", "00", timezone);
  const dayEndExclusive = zonedDateTimeToUtc(dayKey, "23", "59", timezone);
  // Inclusive end for selecting 23:59 when next is null / next day
  const minExclusive =
    timeWindow.previous_measured_at != null &&
    timeWindow.previous_measured_at.getTime() >= dayStart.getTime()
      ? minutesOfDay(timeWindow.previous_measured_at, timezone)
      : -1;
  const maxExclusive =
    timeWindow.next_measured_at != null &&
    timeWindow.next_measured_at.getTime() <= dayEndExclusive.getTime() + 60_000
      ? minutesOfDay(timeWindow.next_measured_at, timezone)
      : 24 * 60;

  const hours: string[] = [];
  for (const hour of ALL_HOURS) {
    const hourStart = Number(hour) * 60;
    const hourEnd = hourStart + 59;
    if (hourEnd > minExclusive && hourStart < maxExclusive) {
      hours.push(hour);
    }
  }

  return {
    hours,
    minutesForHour: (hour: string) => {
      const hourStart = Number(hour) * 60;
      return ALL_MINUTES.filter((minute) => {
        const total = hourStart + Number(minute);
        return total > minExclusive && total < maxExclusive;
      });
    },
  };
}

function TimeSelects({
  id,
  hour,
  minute,
  hours,
  minutes,
  onHourChange,
  onMinuteChange,
}: {
  id?: string;
  hour: string;
  minute: string;
  hours: string[];
  minutes: string[];
  onHourChange: (hour: string) => void;
  onMinuteChange: (minute: string) => void;
}) {
  return (
    <div className="flex items-end gap-2 border-t p-3">
      <Field className="flex-1">
        <FieldLabel htmlFor={id != null ? `${id}-hour` : undefined}>
          Hora
        </FieldLabel>
        <NativeSelect
          id={id != null ? `${id}-hour` : undefined}
          className="w-full"
          value={hour}
          onChange={(event) => {
            onHourChange(event.target.value);
          }}
        >
          {hours.map((option) => (
            <NativeSelectOption key={option} value={option}>
              {option}
            </NativeSelectOption>
          ))}
        </NativeSelect>
      </Field>

      <Field className="flex-1">
        <FieldLabel htmlFor={id != null ? `${id}-minute` : undefined}>
          Minuto
        </FieldLabel>
        <NativeSelect
          id={id != null ? `${id}-minute` : undefined}
          className="w-full"
          value={minute}
          onChange={(event) => {
            onMinuteChange(event.target.value);
          }}
        >
          {minutes.map((option) => (
            <NativeSelectOption key={option} value={option}>
              {option}
            </NativeSelectOption>
          ))}
        </NativeSelect>
      </Field>
    </div>
  );
}

export function MeasuredAtField({
  id,
  value,
  onChange,
  fixedDay,
  timeWindow,
}: MeasuredAtFieldProps) {
  const [open, setOpen] = React.useState(false);
  const { timezone } = useTimezone();
  const { dayKey, hour, minute } = zonedParts(value, timezone);
  const activeDayKey = fixedDay ?? dayKey;
  const selectedDate = dayKeyToCalendarDate(activeDayKey, timezone);

  const { hours, minutesForHour } = allowedHoursMinutes({
    dayKey: activeDayKey,
    timezone,
    timeWindow,
  });
  const minutes = minutesForHour(hour);

  const update = React.useCallback(
    (next: { dayKey?: string; hour?: string; minute?: string }) => {
      onChange(
        zonedDateTimeToUtc(
          next.dayKey ?? activeDayKey,
          next.hour ?? hour,
          next.minute ?? minute,
          timezone,
        ),
      );
    },
    [activeDayKey, hour, minute, onChange, timezone],
  );

  return (
    <Field className="min-w-48 max-w-60">
      <FieldLabel htmlFor={id}>
        {fixedDay != null ? "Hora" : "Data e hora"}
      </FieldLabel>
      <Popover open={open} onOpenChange={setOpen}>
        <PopoverTrigger
          render={
            <Button
              variant="outline"
              id={id}
              className="w-full justify-between font-normal tabular-nums"
            />
          }
        >
          <Time date={value} formatStr="dd/MM/yyyy HH:mm" />
          <ChevronDownIcon data-icon="inline-end" />
        </PopoverTrigger>
        <PopoverContent className="w-auto overflow-hidden p-0" align="start">
          {fixedDay == null ? (
            <Calendar
              mode="single"
              timeZone={timezone}
              captionLayout="dropdown"
              selected={selectedDate}
              defaultMonth={selectedDate}
              onSelect={(date) => {
                if (date == null) return;
                update({ dayKey: calendarDateToDayKey(date, timezone) });
              }}
            />
          ) : (
            <p className="border-b px-3 py-2 text-sm font-medium tabular-nums">
              <Time date={value} formatStr="dd/MM/yyyy" />
            </p>
          )}
          <TimeSelects
            id={id}
            hour={hour}
            minute={minute}
            hours={hours}
            minutes={minutes.length > 0 ? minutes : ALL_MINUTES}
            onHourChange={(nextHour) => {
              const nextMinutes = minutesForHour(nextHour);
              const nextMinute = nextMinutes.includes(minute)
                ? minute
                : (nextMinutes[0] ?? "00");
              update({ hour: nextHour, minute: nextMinute });
            }}
            onMinuteChange={(nextMinute) => {
              update({ minute: nextMinute });
            }}
          />
        </PopoverContent>
      </Popover>
    </Field>
  );
}
