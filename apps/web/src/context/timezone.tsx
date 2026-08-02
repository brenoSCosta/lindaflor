import { CLIENT_TIMEZONE_COOKIE_NAME } from "@lindaflor/shared/constants";
import React from "react";

import { TIMEZONE_OPTIONS } from "@/context/timezone-options";

const COOKIE_MAX_AGE = 31_536_000; // 1 year

function getSystemTimezone(): string {
  return Intl.DateTimeFormat().resolvedOptions().timeZone;
}

function readTimezoneCookie(): string | null {
  if (typeof window === "undefined") return null;
  const match = document.cookie
    .split("; ")
    .find((row) => row.startsWith(`${CLIENT_TIMEZONE_COOKIE_NAME}=`));
  const raw = match?.split("=").slice(1).join("=");
  const value = raw ? decodeURIComponent(raw) : null;
  if (value === null || value === "" || value === "system") return null;
  return value;
}

function ensureTimezoneCookie(): void {
  if (typeof window === "undefined") return;
  if (readTimezoneCookie() != null) return;
  persistEffectiveTimezone(getSystemTimezone());
}

function getClientTimezone(): string {
  if (typeof window === "undefined") {
    return getSystemTimezone();
  }
  ensureTimezoneCookie();
  return readTimezoneCookie() ?? getSystemTimezone();
}

function persistEffectiveTimezone(ianaValue: string): void {
  if (typeof window === "undefined") return;
  const sameSite = "Lax";
  const secure = window.location.protocol === "https:" ? "; Secure" : "";
  document.cookie = `${CLIENT_TIMEZONE_COOKIE_NAME}=${encodeURIComponent(ianaValue)}; path=/; max-age=${COOKIE_MAX_AGE}; SameSite=${sameSite}${secure}`;
}

function getTimezoneLabel(iana: string): string {
  if (iana === "system" || !iana) return "System";
  const found = TIMEZONE_OPTIONS.find((o) => o.value === iana);
  return found?.label ?? iana;
}

type TimezoneContextValue = {
  timezone: string;
  timezoneLabel: string;
  selectedValue: string;
  setTimezone: (ianaValue: string) => void;
};

const TimezoneContext = React.createContext<TimezoneContextValue | null>(null);

function getDefaultTimezoneValue(): TimezoneContextValue {
  const timezone = getClientTimezone();
  const systemTz = getSystemTimezone();
  const selectedValue = timezone === systemTz ? "system" : timezone;
  const timezoneLabel = getTimezoneLabel(selectedValue);
  return {
    timezone,
    timezoneLabel,
    selectedValue,
    setTimezone: () => {},
  };
}

const defaultTimezoneValue = getDefaultTimezoneValue();

export function useTimezone(): TimezoneContextValue {
  const ctx = React.use(TimezoneContext);
  if (!ctx) return defaultTimezoneValue;
  return ctx;
}

export function TimezoneProvider({ children }: { children: React.ReactNode }) {
  const systemTz = getSystemTimezone();
  const [storedValue, setStoredValue] = React.useState<string>(() => {
    if (typeof window === "undefined") return "system";
    const tz = getClientTimezone();
    return tz === systemTz ? "system" : tz;
  });

  const timezone = storedValue === "system" ? getClientTimezone() : storedValue;
  const timezoneLabel = getTimezoneLabel(storedValue);

  React.useEffect(() => {
    persistEffectiveTimezone(timezone);
  }, [timezone]);

  const setTimezone = React.useCallback((ianaValue: string) => {
    if (ianaValue === "system") {
      setStoredValue("system");
      persistEffectiveTimezone(getSystemTimezone());
      return;
    }
    setStoredValue(ianaValue);
    persistEffectiveTimezone(ianaValue);
  }, []);

  const value = React.useMemo<TimezoneContextValue>(
    () => ({
      timezone,
      timezoneLabel,
      selectedValue: storedValue,
      setTimezone,
    }),
    [timezone, timezoneLabel, storedValue, setTimezone],
  );

  return (
    <TimezoneContext.Provider value={value}>
      {children}
    </TimezoneContext.Provider>
  );
}
