import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { useAppAbility } from "@/lib/ability";
import { TankTransferForm } from "@/routes/(auth)/tankages/-components/tank-transfer-form";

interface TankTransferDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  tankId: string;
  day: string;
}

export function TankTransferDialog({
  open,
  onOpenChange,
  tankId,
  day,
}: TankTransferDialogProps) {
  const ability = useAppAbility();
  const canCreate = ability.can("create", "TankTransfers");

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-h-[92vh] overflow-y-auto sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>Transferência</DialogTitle>
          <DialogDescription>
            Informe altura antes/depois e temperaturas. O estoque atualiza com a
            altura depois.
          </DialogDescription>
        </DialogHeader>
        {open ? (
          canCreate ? (
            <TankTransferForm
              key={day}
              tankId={tankId}
              day={day}
              onSubmitted={() => {
                onOpenChange(false);
              }}
            />
          ) : (
            <p className="text-sm text-muted-foreground">
              Você não tem permissão para registrar transferências nesta
              organização.
            </p>
          )
        ) : null}
      </DialogContent>
    </Dialog>
  );
}
