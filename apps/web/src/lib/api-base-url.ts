import { env } from "@lindaflor/env/web";

export function getApiBaseUrl() {
  if (import.meta.env.DEV && typeof window !== "undefined") {
    return window.location.origin;
  }

  return env.VITE_SERVER_URL ?? "http://localhost:9020";
}
