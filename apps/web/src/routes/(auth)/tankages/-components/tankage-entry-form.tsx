import {
  nowInTimezone,
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
import { useMutation, useQuery } from "@tanstack/react-query";
import { Check } from "lucide-react";
import React from "react";
import { toast } from "sonner";
import { z } from "zod";

import { useAppForm } from "@/components/form/hooks";
import { useTimezone } from "@/context/timezone";
import { authClient } from "@/lib/auth-client";
import { orpc, queryClient } from "@/lib/orpc";
import { invalidateTankDay } from "@/routes/(auth)/tankages/-components/invalidate-tank-day";
import { MeasuredAtField } from "@/routes/(auth)/tankages/-components/measured-at-field";

const entrySchema = schema.create.input;
/** Form values always carry a Date; API create still accepts ISO strings via jsonInstantSchema. */
const entryFormSchema = entrySchema.omit({ measured_at: true }).extend({
  measured_at: z.date({ error: "Informe a hora" }),
});

type EntryValues = z.infer<typeof entryFormSchema>;

function defaultMeasuredAt(day: string | undefined, timezone: string): Date {
  if (day == null) return nowInTimezone(timezone);

  const { hour, minute } = zonedParts(new Date(), timezone);
  return zonedDateTimeToUtc(day, hour, minute, timezone);
}

function emptyValues(
  operatorUserId: string,
  tankId: string,
  measurementEquipmentId: string | null,
  timezone: string,
  day?: string,
): EntryValues {
  return {
    tank_id: tankId,
    measured_at: defaultMeasuredAt(day, timezone),
    current_measurement: 0,
    oil_temperature_c: 0,
    ambient_temperature_c: 0,
    observation: "",
    operator_user_id: operatorUserId,
    measurement_equipment_id: measurementEquipmentId,
    latitude: undefined,
    longitude: undefined,
  };
}

const EMPTY_DAY_ROWS: readonly TankageOutput[] = [];

interface TankageEntryFormProps {
  tankId: string;
  day?: string;
  formId?: string;
  onSubmitted?: () => void;
  /** Day bulletin rows already loaded by the parent. */
  dayRows?: readonly TankageOutput[];
  capacityHeightM?: number | null;
  fallbackPreviousHeightM?: number | null;
}

export function TankageEntryForm({
  tankId,
  day,
  formId = "tankage-entry-form",
  onSubmitted,
  dayRows = EMPTY_DAY_ROWS,
  capacityHeightM = null,
  fallbackPreviousHeightM = null,
}: TankageEntryFormProps) {
  const { timezone } = useTimezone();
  const previousTimezoneRef = React.useRef(timezone);
  const { data: session } = authClient.useSession();
  const defaultOperatorId = session?.user.id ?? "";

  const { data: tank } = useQuery({
    ...orpc.tanks.v1.tank.getBy.id.queryOptions({ input: { id: tankId } }),
  });

  const mutation = useMutation(
    orpc.tanks.v1.tankage.create.mutationOptions({
      onError: (error) => toast.error(error.message),
    }),
  );

  const form = useAppForm({
    defaultValues: emptyValues(
      defaultOperatorId,
      tankId,
      tank?.measurement_equipment_id ?? null,
      timezone,
      day,
    ),
    validators: {
      onSubmit: withTankageMeasurementValidation(entryFormSchema, (value) =>
        tankageBoundsFromDayRows({
          rows: dayRows,
          measuredAt: value.measured_at,
          capacityHeightM,
          fallbackPreviousHeightM,
        }),
      ),
    },
    onSubmit: async ({ value }) => {
      const payload = entrySchema.parse(value);
      const bounds = tankageBoundsFromDayRows({
        rows: dayRows,
        measuredAt: payload.measured_at,
        capacityHeightM,
        fallbackPreviousHeightM,
      });
      await mutation.mutateAsync({
        ...payload,
        previous_measurement: bounds.previous_measurement,
      });
      toast.success("Medição registrada");
      await queryClient.invalidateQueries({
        queryKey: orpc.tanks.v1.tankage.list.all.key(),
      });
      if (day != null) {
        await invalidateTankDay({ tankId, operationalDay: day });
      } else {
        await queryClient.invalidateQueries({
          queryKey: orpc.tanks.v1.tankage.listBy.tank.key({
            input: { tank_id: tankId },
          }),
        });
        await queryClient.invalidateQueries({
          queryKey: orpc.tanks.v1.tank.get.snapshot.key({
            input: { id: tankId },
          }),
        });
        await queryClient.invalidateQueries({
          queryKey: orpc.tanks.v1.tank.list.snapshot.key(),
        });
        await queryClient.invalidateQueries({
          queryKey: orpc.tanks.v1.summary.listBy.tank.key({
            input: { tank_id: tankId },
          }),
        });
      }
      if (day == null) {
        form.reset(
          emptyValues(
            payload.operator_user_id || defaultOperatorId,
            tankId,
            tank?.measurement_equipment_id ?? null,
            timezone,
          ),
        );
      }
      onSubmitted?.();
    },
  });

  React.useEffect(() => {
    if (tank == null) return;
    form.setFieldValue("tank_id", tankId);
    form.setFieldValue(
      "measurement_equipment_id",
      tank.measurement_equipment_id,
    );
  }, [form, tank, tankId]);

  React.useEffect(() => {
    const previousTimezone = previousTimezoneRef.current;
    if (previousTimezone === timezone) return;
    const measuredAt = form.state.values.measured_at;
    const parts = zonedParts(measuredAt, previousTimezone);
    previousTimezoneRef.current = timezone;
    form.setFieldValue(
      "measured_at",
      zonedDateTimeToUtc(
        day ?? parts.dayKey,
        parts.hour,
        parts.minute,
        timezone,
      ),
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
              fallbackPreviousHeightM,
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

                <p className="text-xs text-muted-foreground tabular-nums">
                  Altura mínima: {bounds.previous_measurement.toFixed(3)} m
                  {bounds.capacity_height_m != null
                    ? ` · máxima: ${bounds.capacity_height_m.toFixed(3)} m`
                    : " · máxima: sem arqueação vigente"}
                </p>

                <form.AppField
                  name="current_measurement"
                  validators={{
                    onChange: tankageCurrentMeasurementSchema(bounds),
                  }}
                >
                  {(height) => (
                    <div className="space-y-2">
                      <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-3">
                        <height.Field>
                          <height.Label>Altura (m)</height.Label>
                          <height.NumberInput
                            step="0.001"
                            placeholder="0,000"
                          />
                        </height.Field>

                        <form.AppField
                          name="oil_temperature_c"
                          validators={{
                            onChange: entryFormSchema.shape.oil_temperature_c,
                          }}
                        >
                          {(field) => (
                            <field.Field>
                              <field.Label>Óleo (°C)</field.Label>
                              <field.NumberInput step="0.1" placeholder="0,0" />
                              <field.Error />
                            </field.Field>
                          )}
                        </form.AppField>

                        <form.AppField
                          name="ambient_temperature_c"
                          validators={{
                            onChange:
                              entryFormSchema.shape.ambient_temperature_c,
                          }}
                        >
                          {(field) => (
                            <field.Field>
                              <field.Label>Amb. (°C)</field.Label>
                              <field.NumberInput step="0.1" placeholder="0,0" />
                              <field.Error />
                            </field.Field>
                          )}
                        </form.AppField>
                      </div>
                      <height.Error />
                    </div>
                  )}
                </form.AppField>
              </>
            );
          }}
        </form.Subscribe>

        <form.AppField
          name="observation"
          validators={{
            onChange: entryFormSchema.shape.observation,
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

        <div className="flex justify-end">
          <form.Button
            loading={mutation.isPending}
            loadingText="Salvando…"
            className="min-w-36"
          >
            <Check className="size-4" />
            Confirmar
          </form.Button>
        </div>
      </form.AppForm>
    </form>
  );
}
