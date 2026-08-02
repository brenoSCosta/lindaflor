import { NodeToolbar, type NodeToolbarProps } from "@xyflow/react";
import {
  createContext,
  use,
  useMemo,
  useState,
  type ComponentProps,
  type MouseEvent,
} from "react";

import { cn } from "@/lib/utils";

type TooltipContextValue = {
  isVisible: boolean;
  showTooltip: () => void;
  hideTooltip: () => void;
};

const TooltipContext = createContext<TooltipContextValue | null>(null);

function useTooltipContext(componentName: string) {
  const context = use(TooltipContext);
  if (!context) {
    throw new Error(`${componentName} must be used within NodeTooltip`);
  }
  return context;
}

export function NodeTooltip({ children }: ComponentProps<"div">) {
  const [isVisible, setIsVisible] = useState(false);
  const value = useMemo(
    () => ({
      isVisible,
      showTooltip: () => setIsVisible(true),
      hideTooltip: () => setIsVisible(false),
    }),
    [isVisible],
  );

  return (
    <TooltipContext.Provider value={value}>
      <div data-slot="flow-node-tooltip">{children}</div>
    </TooltipContext.Provider>
  );
}

export function NodeTooltipTrigger(props: ComponentProps<"div">) {
  const { showTooltip, hideTooltip } = useTooltipContext("NodeTooltipTrigger");

  function onMouseEnter(event: MouseEvent<HTMLDivElement>) {
    props.onMouseEnter?.(event);
    showTooltip();
  }

  function onMouseLeave(event: MouseEvent<HTMLDivElement>) {
    props.onMouseLeave?.(event);
    hideTooltip();
  }

  return (
    <div {...props} onMouseEnter={onMouseEnter} onMouseLeave={onMouseLeave} />
  );
}

export function NodeTooltipContent({
  children,
  position,
  className,
  ...props
}: NodeToolbarProps) {
  const { isVisible } = useTooltipContext("NodeTooltipContent");

  return (
    <NodeToolbar
      data-slot="flow-node-tooltip-content"
      isVisible={isVisible}
      className={cn(
        "bg-primary text-primary-foreground rounded-sm p-2",
        className,
      )}
      tabIndex={0}
      position={position}
      {...props}
    >
      {children}
    </NodeToolbar>
  );
}
