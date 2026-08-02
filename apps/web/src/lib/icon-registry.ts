import type { FacetIconName } from "@lindaflor/shared/lib/facet-icons";
import {
  ArrowDown,
  ArrowRight,
  ArrowUp,
  Book,
  Bug,
  CheckCircle,
  Circle,
  CircleOff,
  Flag,
  HelpCircle,
  Timer,
} from "lucide-react";

type IconComponent = React.ComponentType<{ className?: string }>;

const ICON_REGISTRY: Record<FacetIconName, IconComponent> = {
  ArrowDown,
  ArrowRight,
  ArrowUp,
  Book,
  Bug,
  CheckCircle,
  Circle,
  CircleOff,
  Flag,
  HelpCircle,
  Timer,
};

export function getIcon(
  name: FacetIconName | undefined,
): IconComponent | undefined {
  if (!name) return undefined;
  return ICON_REGISTRY[name];
}
