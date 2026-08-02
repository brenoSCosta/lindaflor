import React from "react";

import type { DataTableIcons } from "@/components/ui/data-table/core/icons";
import type { DataTableLocalization } from "@/components/ui/data-table/core/localization";

/**
 * App-wide defaults for every table below it. `useDataTable` merges these
 * between the built-in defaults and per-call options:
 *   defaults  <  provider  <  useDataTable({ icons, localization })
 * Handy for setting one icon library or locale across a whole app without
 * passing it to each table.
 */
export interface DataTableConfigContextValue {
  icons?: Partial<DataTableIcons>;
  localization?: Partial<DataTableLocalization>;
}

const DataTableConfigContext = React.createContext<DataTableConfigContextValue>(
  {},
);

export function DataTableConfigProvider({
  icons,
  localization,
  children,
}: DataTableConfigContextValue & { children: React.ReactNode }) {
  const value = React.useMemo(
    () => ({ icons, localization }),
    [icons, localization],
  );
  return (
    <DataTableConfigContext.Provider value={value}>
      {children}
    </DataTableConfigContext.Provider>
  );
}

export function useDataTableConfigContext(): DataTableConfigContextValue {
  return React.use(DataTableConfigContext);
}
