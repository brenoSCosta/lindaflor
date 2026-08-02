import {
  labOilSampleTypeLabels,
  lab_oil_sample_types,
} from "@lindaflor/shared/enums/tankage";
import { nowInTimezone } from "@lindaflor/shared/lib/zoned-datetime";
import {
  schema as labOilAnalysisSchema,
  type LabOilAnalysisOutput,
} from "@lindaflor/shared/schemas/lab-oil-analysis";
import { useMutation } from "@tanstack/react-query";
import { formatInTimeZone } from "date-fns-tz";
import { useState } from "react";
import { toast } from "sonner";
import { z } from "zod";

import {
  FormDialogBody,
  FormDialogSection,
  FormDialogShell,
} from "@/components/form/form-dialog-shell";
import { useAppForm, withForm } from "@/components/form/hooks";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger,
} from "@/components/ui/alert-dialog";
import { Button } from "@/components/ui/button";
import { DialogFooter } from "@/components/ui/dialog";
import { useTimezone } from "@/context/timezone";
import { Can } from "@/lib/ability";
import { orpc, queryClient } from "@/lib/orpc";
import { MeasuredAtField } from "@/routes/(auth)/tankages/-components/measured-at-field";

const formValuesSchema = z.object({
  sample_type: labOilAnalysisSchema.v1.create.input.shape.sample_type,
  collected_at: z.date(),
  issued_at: z.iso.date(),
  certificate_number: z
    .string()
    .min(1, { message: "Informe o número do certificado" })
    .trim(),
  laboratory_name: z
    .string()
    .min(1, { message: "Informe o laboratório" })
    .trim(),
  method_density: z.string(),
  method_basic_sediment_water: z.string(),
  density_at_20c: z
    .number({ error: "Informe a densidade a 20 °C" })
    .positive({ message: "Densidade deve ser maior que zero" }),
  water_and_sediment_percent: z
    .number({ error: "Informe o percentual de água e sedimentos" })
    .min(0, { message: "Água e sedimentos deve ser no mínimo 0%" })
    .max(100, { message: "Água e sedimentos deve ser no máximo 100%" }),
  salinity: z.number().nonnegative().optional(),
});

type FormValues = z.infer<typeof formValuesSchema>;

function emptyFormValues(timezone: string): FormValues {
  return {
    sample_type: "running",
    collected_at: nowInTimezone(timezone),
    issued_at: formatInTimeZone(new Date(), timezone, "yyyy-MM-dd"),
    certificate_number: "",
    laboratory_name: "",
    method_density: "",
    method_basic_sediment_water: "",
    density_at_20c: 0,
    water_and_sediment_percent: 0,
    salinity: undefined,
  };
}

function analysisToFormValues(analysis: LabOilAnalysisOutput): FormValues {
  return {
    sample_type: analysis.sample_type,
    collected_at: analysis.collected_at,
    issued_at: analysis.issued_at,
    certificate_number: analysis.certificate_number,
    laboratory_name: analysis.laboratory_name,
    method_density: analysis.method_density ?? "",
    method_basic_sediment_water: analysis.method_basic_sediment_water ?? "",
    density_at_20c: analysis.density_at_20c,
    water_and_sediment_percent: analysis.water_and_sediment_percent,
    salinity: analysis.salinity ?? undefined,
  };
}

function normalizeOptionalFields(value: FormValues): {
  sample_type: FormValues["sample_type"];
  collected_at: Date;
  issued_at: string;
  certificate_number: string;
  laboratory_name: string;
  method_density: string | null;
  method_basic_sediment_water: string | null;
  density_at_20c: number;
  water_and_sediment_percent: number;
  salinity: number | null;
} {
  return {
    sample_type: value.sample_type,
    collected_at: value.collected_at,
    issued_at: value.issued_at,
    certificate_number: value.certificate_number,
    laboratory_name: value.laboratory_name,
    method_density:
      value.method_density.trim() === "" ? null : value.method_density.trim(),
    method_basic_sediment_water:
      value.method_basic_sediment_water.trim() === ""
        ? null
        : value.method_basic_sediment_water.trim(),
    density_at_20c: value.density_at_20c,
    water_and_sediment_percent: value.water_and_sediment_percent,
    salinity: value.salinity ?? null,
  };
}

const LabAnalysisFields = withForm({
  defaultValues: emptyFormValues("UTC"),
  render: function Render({ form }) {
    return (
      <div className="flex flex-col gap-8">
        <FormDialogSection
          title="Amostra e coleta"
          description="Identifique o tipo de amostra e o momento da coleta no campo."
        >
          <form.AppField name="sample_type">
            {(field) => (
              <field.Field className="sm:col-span-2">
                <field.Label>Tipo de amostra</field.Label>
                <field.Select>
                  <field.SelectTrigger>
                    {labOilSampleTypeLabels[field.state.value]}
                  </field.SelectTrigger>
                  <field.SelectContent>
                    {lab_oil_sample_types.map((type) => (
                      <field.SelectItem key={type} value={type}>
                        {labOilSampleTypeLabels[type]}
                      </field.SelectItem>
                    ))}
                  </field.SelectContent>
                </field.Select>
                <field.Error />
              </field.Field>
            )}
          </form.AppField>

          <form.AppField name="collected_at">
            {(field) => (
              <field.Field className="sm:col-span-2">
                <field.Label>Data e hora da coleta</field.Label>
                <MeasuredAtField
                  id={field.name}
                  value={field.state.value}
                  onChange={(date) => {
                    field.handleChange(date);
                  }}
                />
                <field.Error />
              </field.Field>
            )}
          </form.AppField>
        </FormDialogSection>

        <FormDialogSection
          title="Resultados analíticos"
          description="Valores do laudo usados no cálculo de volumes e barris."
        >
          <form.AppField name="density_at_20c">
            {(field) => (
              <field.Field>
                <field.Label>Densidade a 20 °C (kg/m³)</field.Label>
                <field.NumberInput step="any" placeholder="ex.: 852.3" />
                <field.Error />
              </field.Field>
            )}
          </form.AppField>

          <form.AppField name="water_and_sediment_percent">
            {(field) => (
              <field.Field>
                <field.Label>Água e sedimentos (%)</field.Label>
                <field.NumberInput step="any" placeholder="ex.: 0.8" />
                <field.Error />
              </field.Field>
            )}
          </form.AppField>

          <form.AppField name="salinity">
            {(field) => (
              <field.Field className="sm:col-span-2">
                <field.Label>Salinidade (opcional)</field.Label>
                <field.NumberInput step="any" placeholder="—" />
                <field.Error />
              </field.Field>
            )}
          </form.AppField>
        </FormDialogSection>

        <FormDialogSection
          title="Certificado"
          description="Dados de rastreabilidade do laboratório emissor."
        >
          <form.AppField name="certificate_number">
            {(field) => (
              <field.Field>
                <field.Label>Número do certificado</field.Label>
                <field.Input placeholder="ex.: LAB-441" />
                <field.Error />
              </field.Field>
            )}
          </form.AppField>

          <form.AppField name="laboratory_name">
            {(field) => (
              <field.Field>
                <field.Label>Laboratório</field.Label>
                <field.Input placeholder="Nome do laboratório" />
                <field.Error />
              </field.Field>
            )}
          </form.AppField>

          <form.AppField name="issued_at">
            {(field) => (
              <field.Field className="sm:col-span-2">
                <field.Label>Data de emissão</field.Label>
                <field.Input type="date" />
                <field.Error />
              </field.Field>
            )}
          </form.AppField>
        </FormDialogSection>

        <FormDialogSection
          title="Métodos (opcional)"
          description="Normas ou procedimentos citados no certificado."
        >
          <form.AppField name="method_density">
            {(field) => (
              <field.Field>
                <field.Label>Método densidade</field.Label>
                <field.Input placeholder="ex.: ASTM D5002" />
                <field.Error />
              </field.Field>
            )}
          </form.AppField>

          <form.AppField name="method_basic_sediment_water">
            {(field) => (
              <field.Field>
                <field.Label>Método água e sedimentos</field.Label>
                <field.Input placeholder="ex.: ASTM D4007" />
                <field.Error />
              </field.Field>
            )}
          </form.AppField>
        </FormDialogSection>
      </div>
    );
  },
});

interface LabAnalysisFormDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  tankId: string;
  analysis: LabOilAnalysisOutput | null;
}

export function LabAnalysisFormDialog({
  open,
  onOpenChange,
  tankId,
  analysis,
}: LabAnalysisFormDialogProps) {
  const isEdit = analysis != null;

  return (
    <FormDialogShell
      open={open}
      onOpenChange={onOpenChange}
      title={
        isEdit ? "Editar análise de laboratório" : "Nova análise de laboratório"
      }
      description="Registre os resultados do laudo para este tanque. Campos obrigatórios devem estar alinhados ao certificado emitido pelo laboratório."
    >
      {open ? (
        isEdit ? (
          <UpdateLabAnalysisForm
            key={analysis.id}
            analysis={analysis}
            onOpenChange={onOpenChange}
          />
        ) : (
          <CreateLabAnalysisForm
            key="new"
            tankId={tankId}
            onOpenChange={onOpenChange}
          />
        )
      ) : null}
    </FormDialogShell>
  );
}

function CreateLabAnalysisForm({
  tankId,
  onOpenChange,
}: {
  tankId: string;
  onOpenChange: (open: boolean) => void;
}) {
  const { timezone } = useTimezone();

  const createMutation = useMutation(
    orpc.labOilAnalyses.v1.create.mutationOptions({
      onSuccess: async () => {
        toast.success("Análise cadastrada com sucesso");
        await queryClient.invalidateQueries({
          queryKey: orpc.labOilAnalyses.v1.listByTank.key(),
        });
        onOpenChange(false);
      },
      onError: (error) => {
        toast.error(error.message);
      },
    }),
  );

  const form = useAppForm({
    defaultValues: emptyFormValues(timezone),
    validators: {
      onSubmit: formValuesSchema,
    },
    onSubmit: ({ value }) => {
      const normalized = normalizeOptionalFields(value);
      createMutation.mutate({
        tank_id: tankId,
        ...normalized,
      });
    },
  });

  return (
    <form
      className="flex min-h-0 flex-1 flex-col"
      action={async () => {
        await form.handleSubmit();
      }}
    >
      <form.AppForm>
        <FormDialogBody>
          <LabAnalysisFields form={form} />
        </FormDialogBody>
        <DialogFooter className="mt-0 shrink-0">
          <Button
            type="button"
            variant="outline"
            onClick={() => {
              onOpenChange(false);
            }}
          >
            Cancelar
          </Button>
          <Button type="submit" disabled={createMutation.isPending}>
            {createMutation.isPending ? "Salvando…" : "Salvar"}
          </Button>
        </DialogFooter>
      </form.AppForm>
    </form>
  );
}

function UpdateLabAnalysisForm({
  analysis,
  onOpenChange,
}: {
  analysis: LabOilAnalysisOutput;
  onOpenChange: (open: boolean) => void;
}) {
  const [deleteOpen, setDeleteOpen] = useState(false);

  const updateMutation = useMutation(
    orpc.labOilAnalyses.v1.update.mutationOptions({
      onSuccess: async () => {
        toast.success("Análise atualizada com sucesso");
        await queryClient.invalidateQueries({
          queryKey: orpc.labOilAnalyses.v1.listByTank.key(),
        });
        onOpenChange(false);
      },
      onError: (error) => {
        toast.error(error.message);
      },
    }),
  );

  const deleteMutation = useMutation(
    orpc.labOilAnalyses.v1.delete.mutationOptions({
      onSuccess: async () => {
        toast.success("Análise excluída");
        setDeleteOpen(false);
        await queryClient.invalidateQueries({
          queryKey: orpc.labOilAnalyses.v1.listByTank.key(),
        });
        onOpenChange(false);
      },
      onError: (error) => {
        toast.error(error.message);
      },
    }),
  );

  const form = useAppForm({
    defaultValues: analysisToFormValues(analysis),
    validators: {
      onSubmit: formValuesSchema,
    },
    onSubmit: ({ value }) => {
      const normalized = normalizeOptionalFields(value);
      updateMutation.mutate({
        id: analysis.id,
        ...normalized,
      });
    },
  });

  return (
    <form
      className="flex min-h-0 flex-1 flex-col"
      action={async () => {
        await form.handleSubmit();
      }}
    >
      <form.AppForm>
        <FormDialogBody>
          <LabAnalysisFields form={form} />
        </FormDialogBody>
        <DialogFooter className="mt-0 shrink-0 flex-col gap-2 sm:flex-row sm:justify-between">
          <Can I="delete" a="LabOilAnalyses">
            <AlertDialog open={deleteOpen} onOpenChange={setDeleteOpen}>
              <AlertDialogTrigger
                render={(props) => (
                  <Button
                    type="button"
                    variant="destructive"
                    disabled={
                      deleteMutation.isPending || updateMutation.isPending
                    }
                    {...props}
                  >
                    Excluir
                  </Button>
                )}
              />
              <AlertDialogContent>
                <AlertDialogHeader>
                  <AlertDialogTitle>
                    Excluir análise &quot;{analysis.certificate_number}&quot;?
                  </AlertDialogTitle>
                  <AlertDialogDescription>
                    Esta ação não pode ser desfeita. A análise será removida
                    permanentemente.
                  </AlertDialogDescription>
                </AlertDialogHeader>
                <AlertDialogFooter>
                  <AlertDialogCancel disabled={deleteMutation.isPending}>
                    Cancelar
                  </AlertDialogCancel>
                  <AlertDialogAction
                    variant="destructive"
                    disabled={deleteMutation.isPending}
                    onClick={() => {
                      deleteMutation.mutate({ id: analysis.id });
                    }}
                  >
                    {deleteMutation.isPending ? "Excluindo…" : "Excluir"}
                  </AlertDialogAction>
                </AlertDialogFooter>
              </AlertDialogContent>
            </AlertDialog>
          </Can>
          <div className="flex gap-2">
            <Button
              type="button"
              variant="outline"
              onClick={() => {
                onOpenChange(false);
              }}
            >
              Cancelar
            </Button>
            <Can I="update" a="LabOilAnalyses">
              <Button type="submit" disabled={updateMutation.isPending}>
                {updateMutation.isPending ? "Salvando…" : "Salvar"}
              </Button>
            </Can>
          </div>
        </DialogFooter>
      </form.AppForm>
    </form>
  );
}
