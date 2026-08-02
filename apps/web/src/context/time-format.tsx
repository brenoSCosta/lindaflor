import React from "react";

import {
  CLIENT_TIME_FORMAT_COOKIE_NAME,
  coerceTimeFormatStr,
  DEFAULT_TIME_FORMAT,
  type TimeFormatStr,
} from "@/context/time-format-options";

const COOKIE_MAX_AGE = 31_536_000; // 1 year

function getClientTimeFormat(): TimeFormatStr {
  if (typeof window === "undefined") {
    return DEFAULT_TIME_FORMAT;
  }
  const match = document.cookie
    .split("; ")
    .find((row) => row.startsWith(`${CLIENT_TIME_FORMAT_COOKIE_NAME}=`));
  const raw = match?.split("=").slice(1).join("=");
  return coerceTimeFormatStr(raw ? decodeURIComponent(raw) : null);
}

function setClientTimeFormat(value: TimeFormatStr): void {
  const sameSite = "Lax";
  const secure =
    typeof window !== "undefined" && window.location.protocol === "https:"
      ? "; Secure"
      : "";
  document.cookie = `${CLIENT_TIME_FORMAT_COOKIE_NAME}=${encodeURIComponent(value)}; path=/; max-age=${COOKIE_MAX_AGE}; SameSite=${sameSite}${secure}`;
}

type TimeFormatContextValue = {
  formatStr: TimeFormatStr;
  setFormatStr: (value: TimeFormatStr) => void;
};

const TimeFormatContext = React.createContext<TimeFormatContextValue | null>(
  null,
);

function getDefaultTimeFormatValue(): TimeFormatContextValue {
  return {
    formatStr: getClientTimeFormat(),
    setFormatStr: () => {},
  };
}

const defaultTimeFormatValue = getDefaultTimeFormatValue();

export function useTimeFormat(): TimeFormatContextValue {
  const ctx = React.use(TimeFormatContext);
  if (!ctx) return defaultTimeFormatValue;
  return ctx;
}

export function TimeFormatProvider({
  children,
}: {
  children: React.ReactNode;
}) {
  const [storedValue, setStoredValue] = React.useState<TimeFormatStr>(() =>
    getClientTimeFormat(),
  );

  const setFormatStr = React.useCallback((value: TimeFormatStr) => {
    setStoredValue(value);
    setClientTimeFormat(value);
  }, []);

  const value = React.useMemo<TimeFormatContextValue>(
    () => ({ formatStr: storedValue, setFormatStr }),
    [storedValue, setFormatStr],
  );

  return (
    <TimeFormatContext.Provider value={value}>
      {children}
    </TimeFormatContext.Provider>
  );
}
