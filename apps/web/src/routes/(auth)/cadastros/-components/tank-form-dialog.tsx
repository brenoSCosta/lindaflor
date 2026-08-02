import {
  schema,
  type TankOutput,
} from "@lindaflor/shared/schemas/tankage/tanks";
import { useMutation, useQuery } from "@tanstack/react-query";
import { toast } from "sonner";
import { z } from "zod";

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

interface TankFormDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  tank: TankOutput | null;
}

export function TankFormDialog({
  open,
  onOpenChange,
  tank,
}: TankFormDialogProps) {
  const { data: concessions = [] } = useQuery({
    ...orpc.concessions.v1.getAll.queryOptions({
      select: (result) => result.data,
    }),
    enabled: open,
  });
  const { data: installations = [] } = useQuery({
    ...orpc.installations.v1.getAll.queryOptions({
      select: (result) => result.data,
    }),
    enabled: open,
  });
  const { data: equipments = [] } = useQuery({
    ...orpc.measurementEquipments.v1.getAll.queryOptions({
      select: (result) => result.data,
    }),
    enabled: open,
  });

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>
            {tank != null ? "Editar tanque" : "Novo tanque"}
          </DialogTitle>
        </DialogHeader>
        {tank != null ? (
          <UpdateTankForm
            tank={tank}
            onOpenChange={onOpenChange}
            concessions={concessions}
            installations={installations}
            equipments={equipments}
          />
        ) : (
          <CreateTankForm
            onOpenChange={onOpenChange}
            concessions={concessions}
            installations={installations}
            equipments={equipments}
          />
        )}
      </DialogContent>
    </Dialog>
  );
}

function CreateTankForm({
  onOpenChange,
  concessions,
  installations,
  equipments,
}: {
  onOpenChange: (open: boolean) => void;
  concessions: { id: string; name: string }[];
  installations: { id: string; concession_id: string; name: string }[];
  equipments: { id: string; code: string; active: boolean }[];
}) {
  const defaultValues: z.infer<typeof schema.create.input> = {
    tag: "",
    concession_id: "",
    installation_id: "",
    measurement_equipment_id: undefined,
    latitude: undefined,
    longitude: undefined,
  };

  const createMutation = useMutation(
    orpc.tanks.v1.tank.create.mutationOptions({
      onSuccess: async () => {
        toast.success("Tanque criado com sucesso");
        await queryClient.invalidateQueries({
          queryKey: orpc.tanks.v1.tank.list.all.key(),
        });
        closeDialog(false);
      },
      onError: (error) => {
        toast.error(error.message);
      },
    }),
  );

  const form = useAppForm({
    defaultValues,
    validators: {
      onSubmit: schema.create.input,
    },
    onSubmit: ({ value }) => {
      createMutation.mutate(value);
    },
  });

  const closeDialog = (nextOpen: boolean) => {
    onOpenChange(nextOpen);
    if (!nextOpen) form.reset();
  };

  return (
    <form
      action={async () => {
        await form.handleSubmit();
      }}
    >
      <form.AppForm>
        <div className="grid gap-4">
          <form.AppField name="tag">
            {(field) => (
              <field.Field>
                <field.Label>TAG</field.Label>
                <field.Input
                  placeholder="Informe a TAG do tanque"
                  autoCapitalize="characters"
                />
                <field.Error />
              </field.Field>
            )}
          </form.AppField>

          <form.AppField
            name="concession_id"
            listeners={{
              onChange: ({ value, fieldApi }) => {
                const installationId =
                  fieldApi.form.getFieldValue("installation_id");
                const stillValid = installations.some(
                  (installation) =>
                    installation.id === installationId &&
                    installation.concession_id === value,
                );
                if (!stillValid) {
                  fieldApi.form.setFieldValue("installation_id", "");
                }
              },
            }}
          >
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

          <form.Subscribe selector={(state) => state.values.concession_id}>
            {(concessionId) => {
              const installationOptions = installations.filter(
                (installation) => installation.concession_id === concessionId,
              );
              return (
                <form.AppField name="installation_id">
                  {(field) => {
                    const selectedInstallation = installationOptions.find(
                      (installation) => installation.id === field.state.value,
                    );
                    return (
                      <field.Field>
                        <field.Label>Instalação</field.Label>
                        <field.Select disabled={!concessionId}>
                          <field.SelectTrigger>
                            {selectedInstallation?.name}
                          </field.SelectTrigger>
                          <field.SelectContent>
                            {installationOptions.map((installation) => (
                              <field.SelectItem
                                key={installation.id}
                                value={installation.id}
                              >
                                {installation.name}
                              </field.SelectItem>
                            ))}
                          </field.SelectContent>
                        </field.Select>
                        <field.Error />
                      </field.Field>
                    );
                  }}
                </form.AppField>
              );
            }}
          </form.Subscribe>

          <form.AppField name="measurement_equipment_id">
            {(field) => {
              const selectedEquipment = equipments.find(
                (equipment) => equipment.id === field.state.value,
              );
              return (
                <field.Field>
                  <field.Label>Trena</field.Label>
                  <field.Select
                    value={field.state.value ?? "none"}
                    onValueChange={(value) => {
                      if (typeof value !== "string") return;
                      field.handleChange(value === "none" ? null : value);
                    }}
                  >
                    <field.SelectTrigger>
                      {selectedEquipment?.code ?? "Nenhuma"}
                    </field.SelectTrigger>
                    <field.SelectContent>
                      <field.SelectItem value="none">Nenhuma</field.SelectItem>
                      {equipments.flatMap((equipment) =>
                        equipment.active || equipment.id === field.state.value
                          ? [
                              <field.SelectItem
                                key={equipment.id}
                                value={equipment.id}
                              >
                                {equipment.code}
                              </field.SelectItem>,
                            ]
                          : [],
                      )}
                    </field.SelectContent>
                  </field.Select>
                  <field.Error />
                </field.Field>
              );
            }}
          </form.AppField>

          <div className="grid gap-4 sm:grid-cols-2">
            <form.AppField name="latitude">
              {(field) => (
                <field.Field>
                  <field.Label>Latitude</field.Label>
                  <field.Input
                    type="number"
                    step="any"
                    placeholder="Opcional"
                    value={field.state.value ?? ""}
                    onChange={(e) => {
                      const raw = e.target.value;
                      field.handleChange(raw === "" ? null : Number(raw));
                    }}
                  />
                  <field.Error />
                </field.Field>
              )}
            </form.AppField>
            <form.AppField name="longitude">
              {(field) => (
                <field.Field>
                  <field.Label>Longitude</field.Label>
                  <field.Input
                    type="number"
                    step="any"
                    placeholder="Opcional"
                    value={field.state.value ?? ""}
                    onChange={(e) => {
                      const raw = e.target.value;
                      field.handleChange(raw === "" ? null : Number(raw));
                    }}
                  />
                  <field.Error />
                </field.Field>
              )}
            </form.AppField>
          </div>
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
            loading={form.state.isSubmitting || createMutation.isPending}
            loadingText="Salvando…"
          >
            Salvar
          </form.Button>
        </DialogFooter>
      </form.AppForm>
    </form>
  );
}

function UpdateTankForm({
  tank,
  onOpenChange,
  concessions,
  installations,
  equipments,
}: {
  tank: TankOutput;
  onOpenChange: (open: boolean) => void;
  concessions: { id: string; name: string }[];
  installations: { id: string; concession_id: string; name: string }[];
  equipments: { id: string; code: string; active: boolean }[];
}) {
  const defaultValues: z.infer<typeof schema.update.input> = {
    id: tank.id,
    tag: tank.tag,
    concession_id: tank.concession_id,
    installation_id: tank.installation_id,
    measurement_equipment_id: tank.measurement_equipment_id ?? undefined,
    latitude: tank.latitude ?? undefined,
    longitude: tank.longitude ?? undefined,
  };

  const updateMutation = useMutation(
    orpc.tanks.v1.tank.update.mutationOptions({
      onSuccess: async () => {
        toast.success("Tanque atualizado com sucesso");
        await queryClient.invalidateQueries({
          queryKey: orpc.tanks.v1.tank.list.all.key(),
        });
        closeDialog(false);
      },
      onError: (error) => {
        toast.error(error.message);
      },
    }),
  );

  const form = useAppForm({
    defaultValues,
    validators: {
      onSubmit: schema.update.input,
    },
    onSubmit: ({ value }) => {
      updateMutation.mutate(value);
    },
  });

  const closeDialog = (nextOpen: boolean) => {
    onOpenChange(nextOpen);
    if (!nextOpen) form.reset();
  };

  return (
    <form
      action={async () => {
        await form.handleSubmit();
      }}
    >
      <form.AppForm>
        <div className="grid gap-4">
          <form.AppField name="tag">
            {(field) => (
              <field.Field>
                <field.Label>TAG</field.Label>
                <field.Input
                  placeholder="Informe a TAG do tanque"
                  autoCapitalize="characters"
                />
                <field.Error />
              </field.Field>
            )}
          </form.AppField>

          <form.AppField
            name="concession_id"
            listeners={{
              onChange: ({ value, fieldApi }) => {
                const installationId =
                  fieldApi.form.getFieldValue("installation_id");
                const stillValid = installations.some(
                  (installation) =>
                    installation.id === installationId &&
                    installation.concession_id === value,
                );
                if (!stillValid) {
                  fieldApi.form.setFieldValue("installation_id", "");
                }
              },
            }}
          >
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

          <form.Subscribe selector={(state) => state.values.concession_id}>
            {(concessionId) => {
              const installationOptions = installations.filter(
                (installation) => installation.concession_id === concessionId,
              );
              return (
                <form.AppField name="installation_id">
                  {(field) => {
                    const selectedInstallation = installationOptions.find(
                      (installation) => installation.id === field.state.value,
                    );
                    return (
                      <field.Field>
                        <field.Label>Instalação</field.Label>
                        <field.Select disabled={!concessionId}>
                          <field.SelectTrigger>
                            {selectedInstallation?.name}
                          </field.SelectTrigger>
                          <field.SelectContent>
                            {installationOptions.map((installation) => (
                              <field.SelectItem
                                key={installation.id}
                                value={installation.id}
                              >
                                {installation.name}
                              </field.SelectItem>
                            ))}
                          </field.SelectContent>
                        </field.Select>
                        <field.Error />
                      </field.Field>
                    );
                  }}
                </form.AppField>
              );
            }}
          </form.Subscribe>

          <form.AppField name="measurement_equipment_id">
            {(field) => {
              const selectedEquipment = equipments.find(
                (equipment) => equipment.id === field.state.value,
              );
              return (
                <field.Field>
                  <field.Label>Trena</field.Label>
                  <field.Select
                    value={field.state.value ?? "none"}
                    onValueChange={(value) => {
                      if (typeof value !== "string") return;
                      field.handleChange(value === "none" ? null : value);
                    }}
                  >
                    <field.SelectTrigger>
                      {selectedEquipment?.code ?? "Nenhuma"}
                    </field.SelectTrigger>
                    <field.SelectContent>
                      <field.SelectItem value="none">Nenhuma</field.SelectItem>
                      {equipments.flatMap((equipment) =>
                        equipment.active || equipment.id === field.state.value
                          ? [
                              <field.SelectItem
                                key={equipment.id}
                                value={equipment.id}
                              >
                                {equipment.code}
                              </field.SelectItem>,
                            ]
                          : [],
                      )}
                    </field.SelectContent>
                  </field.Select>
                  <field.Error />
                </field.Field>
              );
            }}
          </form.AppField>

          <div className="grid gap-4 sm:grid-cols-2">
            <form.AppField name="latitude">
              {(field) => (
                <field.Field>
                  <field.Label>Latitude</field.Label>
                  <field.Input
                    type="number"
                    step="any"
                    placeholder="Opcional"
                    value={field.state.value ?? ""}
                    onChange={(e) => {
                      const raw = e.target.value;
                      field.handleChange(raw === "" ? null : Number(raw));
                    }}
                  />
                  <field.Error />
                </field.Field>
              )}
            </form.AppField>
            <form.AppField name="longitude">
              {(field) => (
                <field.Field>
                  <field.Label>Longitude</field.Label>
                  <field.Input
                    type="number"
                    step="any"
                    placeholder="Opcional"
                    value={field.state.value ?? ""}
                    onChange={(e) => {
                      const raw = e.target.value;
                      field.handleChange(raw === "" ? null : Number(raw));
                    }}
                  />
                  <field.Error />
                </field.Field>
              )}
            </form.AppField>
          </div>
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
            loading={form.state.isSubmitting || updateMutation.isPending}
            loadingText="Salvando…"
          >
            Salvar
          </form.Button>
        </DialogFooter>
      </form.AppForm>
    </form>
  );
}
