import {
  schema as installationSchema,
  type InstallationOutput,
} from "@lindaflor/shared/schemas/installation";
import { useMutation, useQuery } from "@tanstack/react-query";
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

const installationFormSchema = installationSchema.v1.create.input;
type InstallationFormValues = z.infer<typeof installationFormSchema>;

interface InstallationFormDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  installation: InstallationOutput | null;
}

export function InstallationFormDialog({
  open,
  onOpenChange,
  installation,
}: InstallationFormDialogProps) {
  const isEditing = installation != null;

  const { data: concessions = [] } = useQuery({
    ...orpc.concessions.v1.getAll.queryOptions({
      select: (result) => result.data,
    }),
    enabled: open,
  });

  const closeDialog = (nextOpen: boolean) => {
    onOpenChange(nextOpen);
    if (!nextOpen) form.reset();
  };

  const createMutation = useMutation(
    orpc.installations.v1.create.mutationOptions({
      onSuccess: async () => {
        toast.success("Instalação criada com sucesso");
        await queryClient.invalidateQueries({
          queryKey: orpc.installations.v1.getAll.key(),
        });
        closeDialog(false);
      },
      onError: (error) => {
        toast.error(error.message);
      },
    }),
  );

  const updateMutation = useMutation(
    orpc.installations.v1.update.mutationOptions({
      onSuccess: async () => {
        toast.success("Instalação atualizada com sucesso");
        await queryClient.invalidateQueries({
          queryKey: orpc.installations.v1.getAll.key(),
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
      name: installation?.name ?? "",
      concession_id: installation?.concession_id ?? "",
    } satisfies InstallationFormValues,
    validators: {
      onSubmit: installationFormSchema,
    },
    onSubmit: ({ value }) => {
      if (isEditing) {
        updateMutation.mutate({ id: installation.id, ...value });
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
            {isEditing ? "Editar instalação" : "Nova instalação"}
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
                    <field.Input placeholder="Nome da instalação" />
                    <field.Error />
                  </field.Field>
                )}
              </form.AppField>

              <form.AppField name="concession_id">
                {(field) => {
                  const selectedConcession = concessions.find(
                    (concession) => concession.id === field.state.value,
                  );
                  return (
                    <field.Field>
                      <field.Label>Concessão</field.Label>
                      <field.Select>
                        <field.SelectTrigger>
                          {selectedConcession?.name}
                        </field.SelectTrigger>
                        <field.SelectContent>
                          {concessions.map((concession) => (
                            <field.SelectItem
                              key={concession.id}
                              value={concession.id}
                            >
                              {concession.name}
                            </field.SelectItem>
                          ))}
                        </field.SelectContent>
                      </field.Select>
                      <field.Error />
                    </field.Field>
                  );
                }}
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
