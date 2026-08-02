import {
  ChevronDownIcon,
  ChevronLeftIcon,
  ChevronRightIcon,
} from "lucide-react";
import React from "react";
import {
  DayPicker,
  getDefaultClassNames,
  useDayPicker,
  type ChevronProps,
  type DayButton,
  type RootProps,
  type WeekNumberProps,
} from "react-day-picker";

import { Button, buttonVariants } from "@/components/ui/button";
import {
  NativeSelect,
  NativeSelectOption,
} from "@/components/ui/native-select";
import { cn } from "@/lib/utils";

interface CalendarDropdownProps {
  options?: Array<{ value: number; label: string; disabled?: boolean }>;
  value?: string | number | readonly string[];
  onChange?: (e: React.ChangeEvent<HTMLSelectElement>) => void;
  disabled?: boolean;
  className?: string;
  "aria-label"?: string;
}

function CalendarDropdown({
  options,
  value,
  onChange,
  disabled,
  className: dropdownClassName,
  "aria-label": ariaLabel,
}: CalendarDropdownProps) {
  const { classNames } = useDayPicker();
  const selectValue = value != null ? String(value) : undefined;
  return (
    <span data-disabled={disabled} className={classNames?.dropdown_root}>
      <NativeSelect
        value={selectValue}
        onChange={onChange}
        disabled={disabled}
        aria-label={ariaLabel}
        className={cn(classNames?.dropdown, dropdownClassName)}
        size="sm"
      >
        {options?.map((option) => (
          <NativeSelectOption
            key={option.value}
            value={option.value}
            disabled={option.disabled}
          >
            {option.label}
          </NativeSelectOption>
        ))}
      </NativeSelect>
    </span>
  );
}

const DEFAULT_START_MONTH = new Date(new Date().getFullYear() - 10, 0);
const DEFAULT_END_MONTH = new Date(new Date().getFullYear() + 10, 0);

function Calendar({
  className,
  classNames,
  showOutsideDays = true,
  captionLayout = "dropdown",
  buttonVariant = "ghost",
  locale,
  formatters,
  components,
  startMonth,
  endMonth,
  ...props
}: React.ComponentProps<typeof DayPicker> & {
  buttonVariant?: React.ComponentProps<typeof Button>["variant"];
}) {
  const defaultClassNames = getDefaultClassNames();

  return (
    <DayPicker
      startMonth={startMonth ?? DEFAULT_START_MONTH}
      endMonth={endMonth ?? DEFAULT_END_MONTH}
      showOutsideDays={showOutsideDays}
      className={cn(
        "group/calendar bg-background p-2 [--cell-radius:var(--radius-md)] [--cell-size:--spacing(7)] in-data-[slot=card-content]:bg-transparent in-data-[slot=popover-content]:bg-transparent",
        "rtl:**:[.rdp-button_next>svg]:rotate-180",
        "rtl:**:[.rdp-button_previous>svg]:rotate-180",
        className,
      )}
      captionLayout={captionLayout}
      locale={locale}
      formatters={{
        formatMonthDropdown: (date) =>
          date.toLocaleString(locale?.code, { month: "short" }),
        ...formatters,
      }}
      classNames={{
        root: cn("w-fit", defaultClassNames.root),
        months: cn(
          "relative flex flex-col gap-4 md:flex-row",
          defaultClassNames.months,
        ),
        month: cn("flex w-full flex-col gap-4", defaultClassNames.month),
        nav: cn(
          "absolute inset-x-0 top-0 flex w-full items-center justify-between gap-1",
          defaultClassNames.nav,
        ),
        button_previous: cn(
          buttonVariants({ variant: buttonVariant }),
          "size-(--cell-size) p-0 select-none aria-disabled:opacity-50",
          defaultClassNames.button_previous,
        ),
        button_next: cn(
          buttonVariants({ variant: buttonVariant }),
          "size-(--cell-size) p-0 select-none aria-disabled:opacity-50",
          defaultClassNames.button_next,
        ),
        month_caption: cn(
          "flex h-(--cell-size) w-full items-center justify-center px-(--cell-size)",
          defaultClassNames.month_caption,
        ),
        dropdowns: cn(
          "flex h-(--cell-size) w-full items-center justify-center gap-2 text-sm font-medium pb-px",
          defaultClassNames.dropdowns,
        ),
        dropdown_root: cn(
          "relative min-w-0 flex-1",
          defaultClassNames.dropdown_root,
        ),
        dropdown: cn(
          "h-full w-full min-w-20 font-medium",
          defaultClassNames.dropdown,
        ),
        caption_label: cn(
          "font-medium select-none",
          captionLayout === "label"
            ? "text-sm"
            : "flex items-center gap-1 rounded-(--cell-radius) text-sm [&>svg]:size-3.5 [&>svg]:text-muted-foreground",
          defaultClassNames.caption_label,
        ),
        month_grid: cn("w-full border-collapse", defaultClassNames.month_grid),
        weekdays: cn("flex", defaultClassNames.weekdays),
        weekday: cn(
          "flex-1 rounded-(--cell-radius) text-[0.8rem] font-normal text-muted-foreground select-none",
          defaultClassNames.weekday,
        ),
        week: cn("mt-2 flex w-full", defaultClassNames.week),
        week_number_header: cn(
          "w-(--cell-size) select-none",
          defaultClassNames.week_number_header,
        ),
        week_number: cn(
          "text-[0.8rem] text-muted-foreground select-none",
          defaultClassNames.week_number,
        ),
        day: cn(
          "group/day relative aspect-square h-full w-full rounded-(--cell-radius) p-0 text-center select-none [&:last-child[data-selected=true]_button]:rounded-r-(--cell-radius)",
          props.showWeekNumber
            ? "[&:nth-child(2)[data-selected=true]_button]:rounded-l-(--cell-radius)"
            : "[&:first-child[data-selected=true]_button]:rounded-l-(--cell-radius)",
          defaultClassNames.day,
        ),
        range_start: cn(
          "relative isolate z-0 rounded-l-(--cell-radius) bg-muted after:absolute after:inset-y-0 after:right-0 after:w-4 after:bg-muted",
          defaultClassNames.range_start,
        ),
        range_middle: cn("rounded-none", defaultClassNames.range_middle),
        range_end: cn(
          "relative isolate z-0 rounded-r-(--cell-radius) bg-muted after:absolute after:inset-y-0 after:left-0 after:w-4 after:bg-muted",
          defaultClassNames.range_end,
        ),
        today: cn(
          "rounded-(--cell-radius) bg-muted text-foreground data-[selected=true]:rounded-none",
          defaultClassNames.today,
        ),
        outside: cn(
          "text-muted-foreground aria-selected:text-muted-foreground",
          defaultClassNames.outside,
        ),
        disabled: cn(
          "text-muted-foreground opacity-50",
          defaultClassNames.disabled,
        ),
        hidden: cn("invisible", defaultClassNames.hidden),
        ...classNames,
      }}
      components={{
        Root: CalendarRoot,
        Chevron: CalendarChevron,
        DayButton: CalendarDayButton,
        WeekNumber: CalendarWeekNumber,
        Dropdown: CalendarDropdown,
        ...components,
      }}
      {...props}
    />
  );
}

function setRootRef(
  ref:
    | { current: HTMLDivElement | null }
    | ((node: HTMLDivElement | null) => void)
    | null
    | undefined,
  node: HTMLDivElement | null,
) {
  if (!ref) return;
  if (typeof ref === "function") {
    ref(node);
  } else {
    ref.current = node;
  }
}

function CalendarRoot({
  className: rootClassName,
  rootRef,
  style: _style,
  ...rootProps
}: RootProps) {
  return (
    <div
      data-slot="calendar"
      ref={(node) => {
        setRootRef(rootRef, node);
      }}
      className={cn(rootClassName)}
      {...rootProps}
    />
  );
}

function CalendarChevron({
  className: chevronClassName,
  orientation,
  ...chevronProps
}: ChevronProps) {
  if (orientation === "left") {
    return (
      <ChevronLeftIcon
        className={cn("size-4", chevronClassName)}
        {...chevronProps}
      />
    );
  }

  if (orientation === "right") {
    return (
      <ChevronRightIcon
        className={cn("size-4", chevronClassName)}
        {...chevronProps}
      />
    );
  }

  return (
    <ChevronDownIcon
      className={cn("size-4", chevronClassName)}
      {...chevronProps}
    />
  );
}

function CalendarWeekNumber({
  children,
  week: _week,
  style: _style,
  ...weekNumberProps
}: WeekNumberProps) {
  return (
    <td {...weekNumberProps}>
      <div className="flex size-(--cell-size) items-center justify-center text-center">
        {children}
      </div>
    </td>
  );
}

function CalendarDayButton({
  className,
  day,
  modifiers,
  style: _style,
  ...buttonProps
}: React.ComponentProps<typeof DayButton>) {
  const { dayPickerProps } = useDayPicker();
  const locale = dayPickerProps.locale;
  const defaultClassNames = getDefaultClassNames();

  return (
    <Button
      variant="ghost"
      size="icon"
      autoFocus={modifiers.focused}
      data-day={day.date.toLocaleDateString(locale?.code)}
      data-selected-single={
        modifiers.selected &&
        !modifiers.range_start &&
        !modifiers.range_end &&
        !modifiers.range_middle
      }
      data-range-start={modifiers.range_start}
      data-range-end={modifiers.range_end}
      data-range-middle={modifiers.range_middle}
      className={cn(
        "relative isolate z-10 flex aspect-square size-auto w-full min-w-(--cell-size) flex-col gap-1 border-0 leading-none font-normal group-data-[focused=true]/day:relative group-data-[focused=true]/day:z-10 group-data-[focused=true]/day:border-ring group-data-[focused=true]/day:ring-[3px] group-data-[focused=true]/day:ring-ring/50 data-[range-end=true]:rounded-(--cell-radius) data-[range-end=true]:rounded-r-(--cell-radius) data-[range-end=true]:bg-primary data-[range-end=true]:text-primary-foreground data-[range-middle=true]:rounded-none data-[range-middle=true]:bg-muted data-[range-middle=true]:text-foreground data-[range-start=true]:rounded-(--cell-radius) data-[range-start=true]:rounded-l-(--cell-radius) data-[range-start=true]:bg-primary data-[range-start=true]:text-primary-foreground data-[selected-single=true]:bg-primary data-[selected-single=true]:text-primary-foreground dark:hover:text-foreground [&>span]:text-xs [&>span]:opacity-70",
        defaultClassNames.day,
        className,
      )}
      {...buttonProps}
    />
  );
}

export { Calendar, CalendarDayButton };
