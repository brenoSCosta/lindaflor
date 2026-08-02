import type { RowData } from "@tanstack/react-table";
import React from "react";

import type { FilterFieldProps } from "@/components/ui/data-table/components/head/filter-variants/shared";
import { getColumnLabel } from "@/components/ui/data-table/helpers/column-label";
import { Slider } from "@/components/ui/slider";

function readSliderPair(
  raw: unknown,
  min: number,
  max: number,
): [number, number] {
  if (!Array.isArray(raw)) return [min, max];
  return [
    typeof raw[0] === "number" ? raw[0] : min,
    typeof raw[1] === "number" ? raw[1] : max,
  ];
}

function normalizePair(
  next: number | readonly number[],
  min: number,
  max: number,
): [number, number] {
  const pair = Array.isArray(next) ? next : [next, next];
  return [pair[0] ?? min, pair[1] ?? max];
}

function pairsEqual(a: [number, number], b: [number, number]): boolean {
  return a[0] === b[0] && a[1] === b[1];
}

export function RangeSliderFilterField<TData extends RowData, TValue>({
  column,
}: FilterFieldProps<TData, TValue>) {
  const facetedMinMax = column.getFacetedMinMaxValues();
  const min = Math.floor(facetedMinMax?.[0] ?? 0);
  const facetMax = Math.ceil(facetedMinMax?.[1] ?? 100);
  const max = min >= facetMax ? min + 1 : facetMax;

  const external = readSliderPair(column.getFilterValue(), min, max);
  const [draft, setDraft] = React.useState<[number, number]>(external);
  const [prevExternal, setPrevExternal] =
    React.useState<[number, number]>(external);

  // Keep the thumb on the local draft while a commit is in flight
  // (`startTransition` means `external` lags behind). Only rewrite draft when
  // the table caught up, the user wasn't mid-edit, or a clear/reset landed.
  if (external[0] !== prevExternal[0] || external[1] !== prevExternal[1]) {
    if (pairsEqual(draft, external)) {
      // Optimistic commit caught up — acknowledge without touching draft.
      setPrevExternal(external);
    } else {
      const wasInSync = pairsEqual(draft, prevExternal);
      const clearedToFullRange = external[0] === min && external[1] === max;
      setPrevExternal(external);
      if (wasInSync || clearedToFullRange) {
        setDraft(external);
      }
    }
  }

  return (
    <div className="flex flex-col gap-1.5 px-1 pt-1">
      <Slider
        min={min}
        max={max}
        step={1}
        value={draft}
        onValueChange={(next) => {
          // Radix always emits `number[]`; Base UI emits `number` for a
          // single-thumb slider. Normalize to a pair so both flavors index
          // safely. Local draft only — table commit waits for release.
          setDraft(normalizePair(next, min, max));
        }}
        onValueCommitted={(next) => {
          const pair = normalizePair(next, min, max);
          // Do not bump prevExternal here: external still holds the old value
          // until the transition commits, and treating that gap as an external
          // change would flash the thumb back to the previous range.
          setDraft(pair);
          React.startTransition(() => {
            column.setFilterValue(
              pair[0] === min && pair[1] === max ? undefined : pair,
            );
          });
        }}
        aria-label={getColumnLabel(column)}
      />
      <div className="flex justify-between text-[10px] text-muted-foreground tabular-nums">
        <span>{draft[0]}</span>
        <span>{draft[1]}</span>
      </div>
    </div>
  );
}
