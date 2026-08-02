import {
  Tour as ArkTour,
  Portal,
  useTour,
  type TourContentProps as ArkTourContentProps,
  type TourBackdropProps,
  type TourDescriptionProps,
  type TourPositionerProps,
  type TourProgressTextProps,
  type TourSpotlightProps,
  type TourStepDetails,
  type TourTitleProps,
  type UseTourReturn,
} from "@ark-ui/react";
import { ark } from "@ark-ui/react/factory";
import { cva, type VariantProps } from "class-variance-authority";
import { ChevronLeftIcon, ChevronRightIcon, XIcon } from "lucide-react";
import React from "react";

import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import { cn } from "@/lib/utils";

export type TourStepType = TourStepDetails;

export { waitForElement, waitForEvent } from "@ark-ui/react";

interface TourProviderValue {
  handleStart: () => void;
  tour: UseTourReturn;
}

const TourContext = React.createContext<TourProviderValue | null>(null);

interface TourProps extends Omit<
  React.ComponentProps<typeof ArkTour.Root>,
  "tour"
> {
  keyboardNavigation?: boolean;
  onStatusChange?: (details: { status: string }) => void;
  onStepChange?: (details: { stepId: string | null }) => void;
  steps: TourStepDetails[];
}

function Tour({
  steps,
  keyboardNavigation,
  onStatusChange,
  onStepChange,
  lazyMount = true,
  unmountOnExit = true,
  ...props
}: TourProps) {
  const tour = useTour({
    steps,
    keyboardNavigation,
    onStatusChange,
    onStepChange,
  });

  const handleStart = React.useCallback(() => {
    tour.start();
  }, [tour]);

  const value = React.useMemo(
    () => ({ tour, handleStart }),
    [tour, handleStart],
  );

  return (
    <TourContext value={value}>
      <ArkTour.Root
        data-slot="tour"
        tour={tour}
        lazyMount={lazyMount}
        unmountOnExit={unmountOnExit}
        {...props}
      />
    </TourContext>
  );
}

function useTourContext() {
  const context = React.use(TourContext);
  if (!context) {
    throw new Error("useTourContext must be used within a Tour.");
  }
  return context;
}

interface TourTriggerProps extends React.ComponentProps<typeof ark.button> {}

function TourTrigger({ onClick, ...props }: TourTriggerProps) {
  const { handleStart } = useTourContext();

  return (
    <ark.button
      data-slot="tour-trigger"
      type="button"
      {...props}
      onClick={(event) => {
        onClick?.(event);
        handleStart();
      }}
    />
  );
}

function TourActionTrigger(
  props: React.ComponentProps<typeof ArkTour.ActionTrigger>,
) {
  return <ArkTour.ActionTrigger data-slot="tour-action-trigger" {...props} />;
}

function TourOverlay({ className, ...props }: TourBackdropProps) {
  return (
    <ArkTour.Backdrop
      data-slot="tour-overlay"
      className={cn(
        "fixed inset-0 isolate z-50 bg-black/10 supports-backdrop-filter:backdrop-blur-xs",
        className,
      )}
      {...props}
    />
  );
}

const tourPositionerVariants = cva(
  "z-50 flex items-center justify-center data-[type=dialog]:fixed data-[type=dialog]:inset-0 data-[type=floating]:fixed data-[type=floating]:inset-0 data-[type=tooltip]:absolute",
  {
    variants: {
      position: {
        top: "",
        center: "",
        bottom: "",
        left: "",
        right: "",
      },
      align: {
        start: "",
        center: "",
        end: "",
      },
    },
    compoundVariants: [
      {
        position: "top",
        align: "start",
        className:
          "data-[type=floating]:items-start data-[type=floating]:justify-start",
      },
      {
        position: "top",
        align: "center",
        className:
          "data-[type=floating]:items-start data-[type=floating]:justify-center",
      },
      {
        position: "top",
        align: "end",
        className:
          "data-[type=floating]:items-start data-[type=floating]:justify-end",
      },
      {
        position: "center",
        className:
          "data-[type=floating]:items-center data-[type=floating]:justify-center",
      },
      {
        position: "bottom",
        align: "start",
        className:
          "data-[type=floating]:items-end data-[type=floating]:justify-start",
      },
      {
        position: "bottom",
        align: "center",
        className:
          "data-[type=floating]:items-end data-[type=floating]:justify-center",
      },
      {
        position: "bottom",
        align: "end",
        className:
          "data-[type=floating]:items-end data-[type=floating]:justify-end",
      },
      {
        position: "left",
        align: "start",
        className:
          "data-[type=floating]:items-start data-[type=floating]:justify-start",
      },
      {
        position: "left",
        align: "center",
        className:
          "data-[type=floating]:items-center data-[type=floating]:justify-start",
      },
      {
        position: "left",
        align: "end",
        className:
          "data-[type=floating]:items-end data-[type=floating]:justify-start",
      },
      {
        position: "right",
        align: "start",
        className:
          "data-[type=floating]:items-start data-[type=floating]:justify-end",
      },
      {
        position: "right",
        align: "center",
        className:
          "data-[type=floating]:items-center data-[type=floating]:justify-end",
      },
      {
        position: "right",
        align: "end",
        className:
          "data-[type=floating]:items-end data-[type=floating]:justify-end",
      },
    ],
    defaultVariants: {
      position: "bottom",
      align: "center",
    },
  },
);

type TourPosition = NonNullable<
  VariantProps<typeof tourPositionerVariants>["position"]
>;
type TourAlign = NonNullable<
  VariantProps<typeof tourPositionerVariants>["align"]
>;

function TourPositioner({
  className,
  position,
  align,
  ...props
}: TourPositionerProps & VariantProps<typeof tourPositionerVariants>) {
  return (
    <ArkTour.Positioner
      data-slot="tour-positioner"
      className={cn(tourPositionerVariants({ position, align }), className)}
      {...props}
    />
  );
}

interface TourContentProps extends ArkTourContentProps {
  showCloseButton?: boolean;
  position?: TourPosition;
  align?: TourAlign;
  progress?: boolean;
}

function TourContent({
  showCloseButton = true,
  position = "bottom",
  align = "end",
  progress = false,
  className,
  children,
  ...props
}: TourContentProps) {
  const { tour } = useTourContext();

  return (
    <Portal>
      <TourOverlay />
      <TourSpotlight />
      <TourPositioner position={position} align={align}>
        <ArkTour.Content
          data-slot="tour-content"
          className={cn(
            "relative z-[calc(50+var(--layer-index,0))] w-full max-w-md",
            "flex flex-col gap-4 rounded-xl border bg-background p-4 shadow-lg",
            "focus:outline-none focus:ring-0",
            "data-[state=closed]:animate-out data-[state=open]:animate-in",
            "data-[state=open]:fade-in-0 data-[state=closed]:fade-out-0",
            "data-[state=open]:zoom-in-95 data-[state=closed]:zoom-out-95",
            "motion-reduce:animate-none!",
            className,
          )}
          {...props}
        >
          {children}
          {showCloseButton && (
            <TourClose asChild>
              <Button
                className="absolute top-4 right-4 border-none opacity-70 hover:opacity-100"
                size="icon-sm"
                variant="ghost"
              >
                <XIcon />
                <span className="sr-only">Close</span>
              </Button>
            </TourClose>
          )}
          {progress && (
            <div className="absolute right-0 bottom-0 left-0 h-1">
              <Progress value={tour.getProgressPercent()} />
            </div>
          )}
        </ArkTour.Content>
      </TourPositioner>
    </Portal>
  );
}

function TourSpotlight({ className, ...props }: TourSpotlightProps) {
  return (
    <ArkTour.Spotlight
      data-slot="tour-spotlight"
      className={cn("z-50 border-2 border-primary", className)}
      {...props}
    />
  );
}

function TourHeader({ className, ...props }: React.ComponentProps<"div">) {
  return (
    <div
      data-slot="tour-header"
      className={cn("flex flex-col gap-2", className)}
      {...props}
    />
  );
}

function TourTitle({ className, ...props }: TourTitleProps) {
  const { tour } = useTourContext();

  return (
    <ArkTour.Title
      data-slot="tour-title"
      className={cn(
        "text-base leading-none font-semibold tracking-tight",
        className,
      )}
      {...props}
    >
      {tour.step?.title}
    </ArkTour.Title>
  );
}

function TourDescription({ className, ...props }: TourDescriptionProps) {
  const { tour } = useTourContext();

  return (
    <ArkTour.Description
      data-slot="tour-description"
      className={cn("text-sm text-muted-foreground", className)}
      {...props}
    >
      {tour.step?.description}
    </ArkTour.Description>
  );
}

function TourProgressText({ className, ...props }: TourProgressTextProps) {
  const { tour } = useTourContext();

  return (
    <ArkTour.ProgressText
      data-slot="tour-progress-text"
      className={cn("text-sm text-muted-foreground", className)}
      {...props}
    >
      {tour.getProgressText()}
    </ArkTour.ProgressText>
  );
}

function TourClose(props: React.ComponentProps<typeof ArkTour.CloseTrigger>) {
  return <ArkTour.CloseTrigger data-slot="tour-close-trigger" {...props} />;
}

function TourBody({ className, ...props }: React.ComponentProps<"div">) {
  return (
    <div
      data-slot="tour-body"
      className={cn("flex flex-col gap-2", className)}
      {...props}
    />
  );
}

function TourFooter({ className, ...props }: React.ComponentProps<"div">) {
  const { tour } = useTourContext();
  const actions = tour.step?.actions ?? [];

  if (actions.length === 0) return null;
  return (
    <ArkTour.Control asChild>
      <div
        data-slot="tour-footer"
        className={cn(
          "-mx-4 -mb-4 flex flex-col-reverse gap-2 rounded-b-xl border-t bg-muted/50 p-4 sm:flex-row sm:justify-end",
          className,
        )}
        {...props}
      />
    </ArkTour.Control>
  );
}

function TourActions({ className, ...props }: React.ComponentProps<"div">) {
  const { tour } = useTourContext();
  const actions = tour.step?.actions ?? [];

  if (actions.length === 0) {
    return null;
  }

  return (
    <ArkTour.Control asChild>
      <div
        data-slot="tour-actions"
        className={cn("flex flex-wrap gap-2", className)}
        {...props}
      >
        {actions.map((action) => (
          <TourActionTrigger action={action} asChild key={action.label}>
            <Button
              size="sm"
              variant={
                action.action === "dismiss" || action.action === "prev"
                  ? "outline"
                  : "default"
              }
            >
              {action.action === "prev" && <ChevronLeftIcon />}
              {action.label}
              {action.action === "next" && <ChevronRightIcon />}
            </Button>
          </TourActionTrigger>
        ))}
      </div>
    </ArkTour.Control>
  );
}

function TourPreviousStep(
  props: Omit<React.ComponentProps<typeof TourActionTrigger>, "action">,
) {
  const { tour } = useTourContext();
  const prevAction = tour.step?.actions?.find(
    (action) => action.action === "prev",
  );

  if (!prevAction) {
    return null;
  }

  return (
    <TourActionTrigger
      data-slot="tour-previous-step"
      action={prevAction}
      asChild
      {...props}
    >
      <Button size="sm" variant="outline">
        <ChevronLeftIcon />
        {prevAction.label}
      </Button>
    </TourActionTrigger>
  );
}

function TourNextStep(
  props: Omit<React.ComponentProps<typeof TourActionTrigger>, "action">,
) {
  const { tour } = useTourContext();
  const action = tour.step?.actions?.find(
    (stepAction) =>
      stepAction.action === "next" || stepAction.action === "dismiss",
  );

  if (!action) {
    return null;
  }

  return (
    <TourActionTrigger
      data-slot="tour-next-step"
      action={action}
      asChild
      {...props}
    >
      <Button size="sm">
        {action.label}
        {action.action === "next" && <ChevronRightIcon />}
      </Button>
    </TourActionTrigger>
  );
}

export {
  Tour,
  TourActions,
  TourActionTrigger,
  TourBody,
  TourClose,
  TourContent,
  TourDescription,
  TourFooter,
  TourHeader,
  TourNextStep,
  TourOverlay,
  TourPositioner,
  TourPreviousStep,
  TourProgressText,
  TourSpotlight,
  TourTitle,
  TourTrigger,
  useTourContext,
};
