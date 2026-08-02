import {
  zonedDateTimeToUtc,
  zonedParts,
} from "@lindaflor/shared/lib/zoned-datetime";
import {
  schema,
  tankageBoundsFromDayRows,
  tankageCurrentMeasurementSchema,
  tankageMeasuredAtSchema,
  withTankageMeasurementValidation,
  type TankageOutput,
} from "@lindaflor/shared/schemas/tankage/tankages";
import { useMutation } from "@tanstack/react-query";
import { Check } from "lucide-react";
import React from "react";
import { toast } from "sonner";
import { z } from "zod";

import { useAppForm } from "@/components/form/hooks";
import { useTimezone } from "@/context/timezone";
import { orpc } from "@/lib/orpc";
import { invalidateTankDay } from "@/routes/(auth)/tankages/-components/invalidate-tank-day";
import { MeasuredAtField } from "@/routes/(auth)/tankages/-components/measured-at-field";

const retreatSchema = schema.retreat.input;

const retreatFormSchema = z.object({
  measured_at: z.date({ error: "Informe a hora" }),
  current_measurement: schema.create.input.shape.current_measurement,
  oil_temperature_c: schema.create.input.shape.oil_temperature_c,
  ambient_temperature_c: schema.create.input.shape.ambient_temperature_c,
  observation: schema.create.input.shape.observation,
  justification: retreatSchema.shape.justification,
});

type RetreatValues = z.infer<typeof retreatFormSchema>;

interface TankageRetreatFormProps {
  tankage: TankageOutput;
  day: string;
  dayRows: readonly TankageOutput[];
  capacityHeightM?: number | null;
  formId?: string;
  onSubmitted?: () => void;
}

export function TankageRetreatForm({
  tankage,
  day,
  dayRows,
  capacityHeightM = null,
  formId = "tankage-retreat-form",
  onSubmitted,
}: TankageRetreatFormProps) {
  const { timezone } = useTimezone();
  const previousTimezoneRef = React.useRef(timezone);

  const mutation = useMutation(
    orpc.tanks.v1.tankage.retreat.mutationOptions({
      onError: (error) => toast.error(error.message),
    }),
  );

  const form = useAppForm({
    defaultValues: {
      measured_at: tankage.measured_at,
      current_measurement: tankage.current_measurement,
      oil_temperature_c: tankage.oil_temperature_c,
      ambient_temperature_c: tankage.ambient_temperature_c,
      observation: tankage.observation,
      justification: "",
    } satisfies RetreatValues,
    validators: {
      onSubmit: withTankageMeasurementValidation(retreatFormSchema, (value) =>
        tankageBoundsFromDayRows({
          rows: dayRows,
          measuredAt: value.measured_at,
          capacityHeightM,
          excludeId: tankage.id,
        }),
      ),
    },
    onSubmit: async ({ value }) => {
      const payload: z.infer<typeof retreatSchema> = {
        id: tankage.id,
        justification: value.justification.trim(),
      };
      if (value.measured_at.getTime() !== tankage.measured_at.getTime()) {
        payload.measured_at = value.measured_at;
      }
      if (value.current_measurement !== tankage.current_measurement) {
        payload.current_measurement = value.current_measurement;
      }
      if (value.oil_temperature_c !== tankage.oil_temperature_c) {
        payload.oil_temperature_c = value.oil_temperature_c;
      }
      if (value.ambient_temperature_c !== tankage.ambient_temperature_c) {
        payload.ambient_temperature_c = value.ambient_temperature_c;
      }
      if (value.observation.trim() !== tankage.observation) {
        payload.observation = value.observation.trim();
      }

      const parsed = retreatSchema.safeParse(payload);
      if (!parsed.success) {
        toast.error(
          parsed.error.issues[0]?.message ?? "Revise os campos do retratamento",
        );
        return;
      }

      await mutation.mutateAsync(parsed.data);
      toast.success("Retratamento registrado");
      await invalidateTankDay({ tankId: tankage.tank_id, operationalDay: day });
      onSubmitted?.();
    },
  });

  React.useEffect(() => {
    const previousTimezone = previousTimezoneRef.current;
    if (previousTimezone === timezone) return;
    const measuredAt = form.state.values.measured_at;
    const parts = zonedParts(measuredAt, previousTimezone);
    previousTimezoneRef.current = timezone;
    form.setFieldValue(
      "measured_at",
      zonedDateTimeToUtc(day, parts.hour, parts.minute, timezone),
    );
  }, [day, form, timezone]);

  return (
    <form
      id={formId}
      className="space-y-4"
      onSubmit={(e) => {
        e.preventDefault();
        void form.handleSubmit();
      }}
    >
      <form.AppForm>
        <form.Subscribe selector={(state) => state.values.measured_at}>
          {(measuredAt) => {
            const bounds = tankageBoundsFromDayRows({
              rows: dayRows,
              measuredAt,
              capacityHeightM,
              excludeId: tankage.id,
            });
            return (
              <>
                <form.AppField
                  name="measured_at"
                  validators={{
                    onChange: tankageMeasuredAtSchema(bounds),
                  }}
                >
                  {(field) => (
                    <field.Field>
                      <MeasuredAtField
                        id={field.name}
                        value={field.state.value}
                        fixedDay={day}
                        timeWindow={bounds}
                        onChange={field.handleChange}
                      />
                      <field.Error />
                    </field.Field>
                  )}
                </form.AppField>

                <form.AppField
                  name="current_measurement"
                  validators={{
                    onChange: tankageCurrentMeasurementSchema(bounds),
                  }}
                >
                  {(field) => (
                    <field.Field>
                      <field.Label>Altura (m)</field.Label>
                      <field.NumberInput step="0.001" placeholder="0,000" />
                      <field.Error />
                    </field.Field>
                  )}
                </form.AppField>
              </>
            );
          }}
        </form.Subscribe>

        <div className="grid gap-3 sm:grid-cols-2">
          <form.AppField
            name="oil_temperature_c"
            validators={{
              onChange: retreatFormSchema.shape.oil_temperature_c,
            }}
          >
            {(field) => (
              <field.Field>
                <field.Label>Temp. Óleo (°C)</field.Label>
                <field.NumberInput step="0.1" placeholder="0,0" />
                <field.Error />
              </field.Field>
            )}
          </form.AppField>

          <form.AppField
            name="ambient_temperature_c"
            validators={{
              onChange: retreatFormSchema.shape.ambient_temperature_c,
            }}
          >
            {(field) => (
              <field.Field>
                <field.Label>Temp. Amb. (°C)</field.Label>
                <field.NumberInput step="0.1" placeholder="0,0" />
                <field.Error />
              </field.Field>
            )}
          </form.AppField>
        </div>

        <form.AppField
          name="observation"
          validators={{
            onChange: retreatFormSchema.shape.observation,
          }}
        >
          {(field) => (
            <field.Field>
              <field.Label>Observação</field.Label>
              <field.Textarea rows={3} placeholder="Observação da medição" />
              <field.Error />
            </field.Field>
          )}
        </form.AppField>

        <form.AppField
          name="justification"
          validators={{
            onChange: retreatFormSchema.shape.justification,
          }}
        >
          {(field) => (
            <field.Field>
              <field.Label>Justificativa do retratamento</field.Label>
              <field.Textarea
                rows={3}
                placeholder="Motivo da correção no boletim aprovado"
              />
              <field.Error />
            </field.Field>
          )}
        </form.AppField>

        <div className="flex justify-end">
          <form.Button
            loading={mutation.isPending}
            loadingText="Salvando…"
            className="min-w-36"
          >
            <Check className="size-4" />
            Confirmar retratamento
          </form.Button>
        </div>
      </form.AppForm>
    </form>
  );
}
