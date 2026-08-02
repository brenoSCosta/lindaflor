import { measurement_equipment_types } from "@lindaflor/shared/enums/tankage";
import {
  MEASUREMENT_EQUIPMENT_TYPE_LABELS,
  schema,
  type MeasurementEquipmentOutput,
} from "@lindaflor/shared/schemas/measurement-equipment";
import { useMutation } from "@tanstack/react-query";
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

interface MeasurementEquipmentFormDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  equipment: MeasurementEquipmentOutput | null;
}

export function MeasurementEquipmentFormDialog({
  open,
  onOpenChange,
  equipment,
}: MeasurementEquipmentFormDialogProps) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-h-[90vh] overflow-y-auto sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>
            {equipment != null ? "Editar trena" : "Nova trena"}
          </DialogTitle>
        </DialogHeader>
        {equipment != null ? (
          <UpdateMeasurementEquipmentForm
            equipment={equipment}
            onOpenChange={onOpenChange}
          />
        ) : (
          <CreateMeasurementEquipmentForm onOpenChange={onOpenChange} />
        )}
      </DialogContent>
    </Dialog>
  );
}

function CreateMeasurementEquipmentForm({
  onOpenChange,
}: {
  onOpenChange: (open: boolean) => void;
}) {
  const defaultValues: z.infer<typeof schema.v1.create.input> = {
    code: "",
    type: "manual",
    active: true,
    description: undefined,
    length_m: undefined,
    reference_height_m: undefined,
    manufacturer: undefined,
    serial_number: undefined,
    calibrated_at: undefined,
    calibration_valid_until: undefined,
  };

  const createMutation = useMutation(
    orpc.measurementEquipments.v1.create.mutationOptions({
      onSuccess: async () => {
        toast.success("Trena criada com sucesso");
        await queryClient.invalidateQueries({
          queryKey: orpc.measurementEquipments.v1.getAll.key(),
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
      onSubmit: schema.v1.create.input,
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
          <form.AppField name="code">
            {(field) => (
              <field.Field>
                <field.Label>Código</field.Label>
                <field.Input
                  placeholder="Ex.: TR-001"
                  autoCapitalize="characters"
                />
                <field.Error />
              </field.Field>
            )}
          </form.AppField>

          <form.AppField name="description">
            {(field) => (
              <field.Field>
                <field.Label>Descrição</field.Label>
                <field.Input placeholder="Opcional" />
                <field.Error />
              </field.Field>
            )}
          </form.AppField>

          <form.AppField name="type">
            {(field) => (
              <field.Field>
                <field.Label>Tipo</field.Label>
                <field.Select>
                  <field.SelectTrigger>
                    {field.state.value
                      ? MEASUREMENT_EQUIPMENT_TYPE_LABELS[field.state.value]
                      : undefined}
                  </field.SelectTrigger>
                  <field.SelectContent>
                    {measurement_equipment_types.map((type) => (
                      <field.SelectItem key={type} value={type}>
                        {MEASUREMENT_EQUIPMENT_TYPE_LABELS[type]}
                      </field.SelectItem>
                    ))}
                  </field.SelectContent>
                </field.Select>
                <field.Error />
              </field.Field>
            )}
          </form.AppField>

          <div className="grid gap-4 sm:grid-cols-2">
            <form.AppField name="length_m">
              {(field) => (
                <field.Field>
                  <field.Label>Comprimento (m)</field.Label>
                  <field.NumberInput step="0.01" placeholder="Ex.: 20" />
                  <field.Error />
                </field.Field>
              )}
            </form.AppField>
            <form.AppField name="reference_height_m">
              {(field) => (
                <field.Field>
                  <field.Label>Altura de referência (m)</field.Label>
                  <field.NumberInput step="0.01" placeholder="Opcional" />
                  <field.Error />
                </field.Field>
              )}
            </form.AppField>
          </div>

          <div className="grid gap-4 sm:grid-cols-2">
            <form.AppField name="manufacturer">
              {(field) => (
                <field.Field>
                  <field.Label>Fabricante</field.Label>
                  <field.Input placeholder="Opcional" />
                  <field.Error />
                </field.Field>
              )}
            </form.AppField>
            <form.AppField name="serial_number">
              {(field) => (
                <field.Field>
                  <field.Label>Número de série</field.Label>
                  <field.Input placeholder="Opcional" />
                  <field.Error />
                </field.Field>
              )}
            </form.AppField>
          </div>

          <div className="grid gap-4 sm:grid-cols-2">
            <form.AppField name="calibrated_at">
              {(field) => (
                <field.Field>
                  <field.Label>Calibrada em</field.Label>
                  <field.Input
                    type="date"
                    value={field.state.value ?? ""}
                    onChange={(e) =>
                      field.handleChange(
                        e.target.value === "" ? undefined : e.target.value,
                      )
                    }
                  />
                  <field.Error />
                </field.Field>
              )}
            </form.AppField>
            <form.AppField name="calibration_valid_until">
              {(field) => (
                <field.Field>
                  <field.Label>Validade da calibração</field.Label>
                  <field.Input
                    type="date"
                    value={field.state.value ?? ""}
                    onChange={(e) =>
                      field.handleChange(
                        e.target.value === "" ? undefined : e.target.value,
                      )
                    }
                  />
                  <field.Error />
                </field.Field>
              )}
            </form.AppField>
          </div>

          <form.AppField name="active">
            {(field) => (
              <field.Field
                orientation="horizontal"
                className="flex-row items-center justify-between rounded-lg border p-3"
              >
                <field.Label className="flex-1">Trena ativa</field.Label>
                <field.Switch />
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

function UpdateMeasurementEquipmentForm({
  equipment,
  onOpenChange,
}: {
  equipment: MeasurementEquipmentOutput;
  onOpenChange: (open: boolean) => void;
}) {
  const defaultValues: z.infer<typeof schema.v1.update.input> = {
    id: equipment.id,
    code: equipment.code,
    type: equipment.type,
    active: equipment.active,
    description: equipment.description ?? undefined,
    length_m: equipment.length_m ?? undefined,
    reference_height_m: equipment.reference_height_m ?? undefined,
    manufacturer: equipment.manufacturer ?? undefined,
    serial_number: equipment.serial_number ?? undefined,
    calibrated_at: equipment.calibrated_at ?? undefined,
    calibration_valid_until: equipment.calibration_valid_until ?? undefined,
  };

  const updateMutation = useMutation(
    orpc.measurementEquipments.v1.update.mutationOptions({
      onSuccess: async () => {
        toast.success("Trena atualizada com sucesso");
        await queryClient.invalidateQueries({
          queryKey: orpc.measurementEquipments.v1.getAll.key(),
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
      onSubmit: schema.v1.update.input,
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
          <form.AppField name="code">
            {(field) => (
              <field.Field>
                <field.Label>Código</field.Label>
                <field.Input
                  placeholder="Ex.: TR-001"
                  autoCapitalize="characters"
                />
                <field.Error />
              </field.Field>
            )}
          </form.AppField>

          <form.AppField name="description">
            {(field) => (
              <field.Field>
                <field.Label>Descrição</field.Label>
                <field.Input placeholder="Opcional" />
                <field.Error />
              </field.Field>
            )}
          </form.AppField>

          <form.AppField name="type">
            {(field) => (
              <field.Field>
                <field.Label>Tipo</field.Label>
                <field.Select>
                  <field.SelectTrigger>
                    {field.state.value
                      ? MEASUREMENT_EQUIPMENT_TYPE_LABELS[field.state.value]
                      : undefined}
                  </field.SelectTrigger>
                  <field.SelectContent>
                    {measurement_equipment_types.map((type) => (
                      <field.SelectItem key={type} value={type}>
                        {MEASUREMENT_EQUIPMENT_TYPE_LABELS[type]}
                      </field.SelectItem>
                    ))}
                  </field.SelectContent>
                </field.Select>
                <field.Error />
              </field.Field>
            )}
          </form.AppField>

          <div className="grid gap-4 sm:grid-cols-2">
            <form.AppField name="length_m">
              {(field) => (
                <field.Field>
                  <field.Label>Comprimento (m)</field.Label>
                  <field.NumberInput step="0.01" placeholder="Ex.: 20" />
                  <field.Error />
                </field.Field>
              )}
            </form.AppField>
            <form.AppField name="reference_height_m">
              {(field) => (
                <field.Field>
                  <field.Label>Altura de referência (m)</field.Label>
                  <field.NumberInput step="0.01" placeholder="Opcional" />
                  <field.Error />
                </field.Field>
              )}
            </form.AppField>
          </div>

          <div className="grid gap-4 sm:grid-cols-2">
            <form.AppField name="manufacturer">
              {(field) => (
                <field.Field>
                  <field.Label>Fabricante</field.Label>
                  <field.Input placeholder="Opcional" />
                  <field.Error />
                </field.Field>
              )}
            </form.AppField>
            <form.AppField name="serial_number">
              {(field) => (
                <field.Field>
                  <field.Label>Número de série</field.Label>
                  <field.Input placeholder="Opcional" />
                  <field.Error />
                </field.Field>
              )}
            </form.AppField>
          </div>

          <div className="grid gap-4 sm:grid-cols-2">
            <form.AppField name="calibrated_at">
              {(field) => (
                <field.Field>
                  <field.Label>Calibrada em</field.Label>
                  <field.Input
                    type="date"
                    value={field.state.value ?? ""}
                    onChange={(e) =>
                      field.handleChange(
                        e.target.value === "" ? undefined : e.target.value,
                      )
                    }
                  />
                  <field.Error />
                </field.Field>
              )}
            </form.AppField>
            <form.AppField name="calibration_valid_until">
              {(field) => (
                <field.Field>
                  <field.Label>Validade da calibração</field.Label>
                  <field.Input
                    type="date"
                    value={field.state.value ?? ""}
                    onChange={(e) =>
                      field.handleChange(
                        e.target.value === "" ? undefined : e.target.value,
                      )
                    }
                  />
                  <field.Error />
                </field.Field>
              )}
            </form.AppField>
          </div>

          <form.AppField name="active">
            {(field) => (
              <field.Field
                orientation="horizontal"
                className="flex-row items-center justify-between rounded-lg border p-3"
              >
                <field.Label className="flex-1">Trena ativa</field.Label>
                <field.Switch />
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
