/** Lucide icon names allowed in facet metadata.
 * Must match components registered on the frontend.
 * If one of them is missing some key, typescript will complain
 * */
export const FACET_ICON_NAMES = [
  "ArrowDown",
  "ArrowRight",
  "ArrowUp",
  "Book",
  "Bug",
  "CheckCircle",
  "Circle",
  "CircleOff",
  "Flag",
  "HelpCircle",
  "Timer",
] as const;

export type FacetIconName = (typeof FACET_ICON_NAMES)[number];
