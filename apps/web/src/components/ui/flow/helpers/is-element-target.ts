export function isElementTarget(target: EventTarget | null): target is Element {
  return target instanceof Element;
}
