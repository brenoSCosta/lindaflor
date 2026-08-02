import {
  STATE_VALUES,
  schema as concessionSchema,
  type ConcessionOutput,
} from "@lindaflor/shared/schemas/concession";
import { useMutation } from "@tanstack/react-query";
import { toast } from "sonner";
import type { z } from "zod";

import { useAppForm } from "@/components/form/hooks";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { orpc, queryClient } from "@/lib/orpc";

const concessionFormSchema = concessionSchema.v1.create.input;
type ConcessionFormValues = z.infer<typeof concessionFormSchema>;

const isStateValue = (value: string): value is ConcessionFormValues["state"] =>
  (STATE_VALUES as readonly string[]).includes(value);

interface ConcessionFormDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  concession: ConcessionOutput | null;
}

export function ConcessionFormDialog({
  open,
  onOpenChange,
  concession,
}: ConcessionFormDialogProps) {
  const isEditing = concession != null;

  const closeDialog = (nextOpen: boolean) => {
    onOpenChange(nextOpen);
    if (!nextOpen) form.reset();
  };

  const createMutation = useMutation(
    orpc.concessions.v1.create.mutationOptions({
      onSuccess: async () => {
        toast.success("Concessão criada com sucesso");
        await queryClient.invalidateQueries({
          queryKey: orpc.concessions.v1.getAll.key(),
        });
        closeDialog(false);
      },
      onError: (error) => {
        toast.error(error.message);
      },
    }),
  );

  const updateMutation = useMutation(
    orpc.concessions.v1.update.mutationOptions({
      onSuccess: async () => {
        toast.success("Concessão atualizada com sucesso");
        await queryClient.invalidateQueries({
          queryKey: orpc.concessions.v1.getAll.key(),
        });
        closeDialog(false);
      },
      onError: (error) => {
        toast.error(error.message);
      },
    }),
  );

  const form = useAppForm({
    defaultValues: {
      name: concession?.name ?? "",
      state:
        concession != null && isStateValue(concession.state)
          ? concession.state
          : STATE_VALUES[0],
    } satisfies ConcessionFormValues,
    validators: {
      onSubmit: concessionFormSchema,
    },
    onSubmit: ({ value }) => {
      if (isEditing) {
        updateMutation.mutate({ id: concession.id, ...value });
      } else {
        createMutation.mutate(value);
      }
    },
  });

  const isSubmitting = createMutation.isPending || updateMutation.isPending;

  return (
    <Dialog open={open} onOpenChange={closeDialog}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>
            {isEditing ? "Editar concessão" : "Nova concessão"}
          </DialogTitle>
        </DialogHeader>

        <form
          action={async () => {
            await form.handleSubmit();
          }}
        >
          <form.AppForm>
            <div className="grid gap-4">
              <form.AppField name="name">
                {(field) => (
                  <field.Field>
                    <field.Label>Nome</field.Label>
                    <field.Input placeholder="Nome da concessão" />
                    <field.Error />
                  </field.Field>
                )}
              </form.AppField>

              <form.AppField name="state">
                {(field) => (
                  <field.Field>
                    <field.Label>Estado</field.Label>
                    <field.Select>
                      <field.SelectTrigger />
                      <field.SelectContent>
                        {STATE_VALUES.map((state) => (
                          <field.SelectItem key={state} value={state}>
                            {state}
                          </field.SelectItem>
                        ))}
                      </field.SelectContent>
                    </field.Select>
                    <field.Error />
                  </field.Field>
                )}
              </form.AppField>
            </div>

            <DialogFooter className="mt-6">
              <Button
                type="button"
                variant="outline"
                onClick={() => closeDialog(false)}
              >
                Cancelar
              </Button>
              <form.Button
                loading={form.state.isSubmitting || isSubmitting}
                loadingText="Salvando…"
              >
                Salvar
              </form.Button>
            </DialogFooter>
          </form.AppForm>
        </form>
      </DialogContent>
    </Dialog>
  );
}
