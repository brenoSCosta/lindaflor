import type { TankageOutput } from "@lindaflor/shared/schemas/tankage/tankages";

import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { TankageEntryForm } from "@/routes/(auth)/tankages/-components/tankage-entry-form";

interface TankageEntryDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  tankId: string;
  day?: string;
  dayRows?: readonly TankageOutput[];
  capacityHeightM?: number | null;
  fallbackPreviousHeightM?: number | null;
}

export function TankageEntryDialog({
  open,
  onOpenChange,
  tankId,
  day,
  dayRows,
  capacityHeightM,
  fallbackPreviousHeightM,
}: TankageEntryDialogProps) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-h-[92vh] overflow-y-auto sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>
            {day != null ? "Medição do boletim" : "Nova medição"}
          </DialogTitle>
          <DialogDescription>
            {day != null
              ? "Lance a medição deste boletim. O detalhamento atualiza na tabela."
              : "Cadastro rápido. O detalhamento fica na tabela do dia."}
          </DialogDescription>
        </DialogHeader>
        {open ? (
          <TankageEntryForm
            key={day ?? "new"}
            tankId={tankId}
            day={day}
            dayRows={dayRows}
            capacityHeightM={capacityHeightM}
            fallbackPreviousHeightM={fallbackPreviousHeightM}
            onSubmitted={() => {
              onOpenChange(false);
            }}
          />
        ) : null}
      </DialogContent>
    </Dialog>
  );
}
