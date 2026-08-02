export function flowNodeLabel(data: unknown, fallback: string): string {
  if (
    data !== null &&
    typeof data === "object" &&
    "label" in data &&
    typeof data.label === "string"
  ) {
    return data.label;
  }
  return fallback;
}
