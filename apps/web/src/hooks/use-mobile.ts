import React from "react";

const MOBILE_BREAKPOINT = 768;

const QUERY = `(max-width: ${MOBILE_BREAKPOINT - 1}px)`;

function getMatchMedia() {
  if (typeof window === "undefined") return null;
  return window.matchMedia(QUERY);
}

function subscribe(callback: () => void) {
  const mql = getMatchMedia();
  if (!mql) return () => {};
  mql.addEventListener("change", callback);
  return () => mql.removeEventListener("change", callback);
}

function getSnapshot() {
  const mql = getMatchMedia();
  return mql?.matches ?? false;
}

function getServerSnapshot() {
  return false;
}

export function useIsMobile() {
  return React.useSyncExternalStore(subscribe, getSnapshot, getServerSnapshot);
}
