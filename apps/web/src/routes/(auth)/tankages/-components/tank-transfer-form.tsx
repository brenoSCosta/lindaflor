import {
  nowInTimezone,
  zonedDateTimeToUtc,
  zonedParts,
} from "@lindaflor/shared/lib/zoned-datetime";
import { schema } from "@lindaflor/shared/schemas/tankage/transfers";
import { useMutation, useQuery } from "@tanstack/react-query";
import { Check } from "lucide-react";
import React from "react";
import { toast } from "sonner";
import { z } from "zod";

import { useAppForm } from "@/components/form/hooks";
import { useTimezone } from "@/context/timezone";
import { authClient } from "@/lib/auth-client";
import { orpc } from "@/lib/orpc";
import { invalidateTankDay } from "@/routes/(auth)/tankages/-components/invalidate-tank-day";
import { MeasuredAtField } from "@/routes/(auth)/tankages/-components/measured-at-field";

const heightSchema = z.number().min(0, {
  message: "Informe a altura",
});

const createSchema = schema.create.input;
type FormValues = z.infer<typeof createSchema>;

function defaultTransferredAt(day: string | undefined, timezone: string): Date {
  if (day == null) return nowInTimezone(timezone);
  const { hour, minute } = zonedParts(new Date(), timezone);
  return zonedDateTimeToUtc(day, hour, minute, timezone);
}

function emptyValues(args: {
  operatorUserId: string;
  tankId: string;
  measurementEquipmentId: string | null;
  heightBefore: number;
  timezone: string;
  day?: string;
}): FormValues {
  return {
    tank_id: args.tankId,
    transferred_at: defaultTransferredAt(args.day, args.timezone),
    height_before_m: args.heightBefore,
    height_after_m: 0,
    oil_temperature_c: 0,
    ambient_temperature_c: 0,
    destination_label: "",
    observation: "",
    operator_user_id: args.operatorUserId,
    measurement_equipment_id: args.measurementEquipmentId,
  };
}

interface TankTransferFormProps {
  tankId: string;
  day: string;
  formId?: string;
  onSubmitted?: () => void;
}

export function TankTransferForm({
  tankId,
  day,
  formId = "tank-transfer-form",
  onSubmitted,
}: TankTransferFormProps) {
  const { timezone } = useTimezone();
  const previousTimezoneRef = React.useRef(timezone);
  const { data: session } = authClient.useSession();
  const defaultOperatorId = session?.user.id ?? "";

  const { data: tank } = useQuery({
    ...orpc.tanks.v1.tank.getBy.id.queryOptions({ input: { id: tankId } }),
  });

  const { data: recentResult } = useQuery({
    ...orpc.tanks.v1.tankage.listBy.tank.queryOptions({
      input: { tank_id: tankId },
    }),
  });
  const heightBeforeDefault = recentResult?.data[0]?.current_measurement ?? 0;

  const mutation = useMutation(
    orpc.tanks.v1.transfer.create.mutationOptions({
      onError: (error) => toast.error(error.message),
    }),
  );

  const form = useAppForm({
    defaultValues: emptyValues({
      operatorUserId: defaultOperatorId,
      tankId,
      measurementEquipmentId: tank?.measurement_equipment_id ?? null,
      heightBefore: heightBeforeDefault,
      timezone,
      day,
    }),
    validators: {
      onSubmit: createSchema,
    },
    onSubmit: async ({ value }) => {
      const parsed = createSchema.safeParse({
        ...value,
        destination_label:
          value.destination_label == null ||
          value.destination_label.trim() === ""
            ? null
            : value.destination_label.trim(),
      });
      if (!parsed.success) {
        const first = parsed.error.issues[0]?.message;
        toast.error(first ?? "Revise os campos da transferência");
        return;
      }
      await mutation.mutateAsync(parsed.data);
      toast.success("Transferência registrada");
      await invalidateTankDay({ tankId, operationalDay: day });
      form.reset(
        emptyValues({
          operatorUserId: defaultOperatorId,
          tankId,
          measurementEquipmentId: tank?.measurement_equipment_id ?? null,
          heightBefore: heightBeforeDefault,
          timezone,
          day,
        }),
      );
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
    if (defaultOperatorId === "") return;
    form.setFieldValue("operator_user_id", defaultOperatorId);
  }, [defaultOperatorId, form]);

  React.useEffect(() => {
    form.setFieldValue("height_before_m", heightBeforeDefault);
  }, [form, heightBeforeDefault]);

  React.useEffect(() => {
    const previousTimezone = previousTimezoneRef.current;
    if (previousTimezone === timezone) return;
    const transferredAt = form.state.values.transferred_at;
    const parts = zonedParts(transferredAt, previousTimezone);
    previousTimezoneRef.current = timezone;
    form.setFieldValue(
      "transferred_at",
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
        <form.AppField
          name="transferred_at"
          validators={{
            onChange: z.date({ message: "Informe a hora" }),
          }}
        >
          {(field) => (
            <field.Field>
              <MeasuredAtField
                id={field.name}
                value={field.state.value}
                fixedDay={day}
                onChange={field.handleChange}
              />
              <field.Error />
            </field.Field>
          )}
        </form.AppField>

        <div className="grid gap-3 sm:grid-cols-2">
          <form.AppField
            name="height_before_m"
            validators={{ onChange: heightSchema }}
          >
            {(field) => (
              <field.Field>
                <field.Label>Altura antes (m)</field.Label>
                <field.NumberInput step="0.001" placeholder="0,000" />
                <field.Error />
              </field.Field>
            )}
          </form.AppField>

          <form.AppField
            name="height_after_m"
            validators={{ onChange: heightSchema }}
          >
            {(field) => (
              <field.Field>
                <field.Label>Altura depois (m)</field.Label>
                <field.NumberInput step="0.001" placeholder="0,000" />
                <field.Error />
              </field.Field>
            )}
          </form.AppField>

          <form.AppField
            name="oil_temperature_c"
            validators={{ onChange: z.number() }}
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
            validators={{ onChange: z.number() }}
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

        <form.AppField name="destination_label">
          {(field) => (
            <field.Field>
              <field.Label>Destino (opcional)</field.Label>
              <field.Input placeholder="ex.: bomba, caminhão, T-12" />
              <field.Error />
            </field.Field>
          )}
        </form.AppField>

        <form.AppField
          name="observation"
          validators={{
            onChange: z.string().min(1, { message: "Informe a observação" }),
          }}
        >
          {(field) => (
            <field.Field>
              <field.Label>Observação</field.Label>
              <field.Textarea
                rows={3}
                placeholder="Observação da transferência"
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
            Confirmar
          </form.Button>
        </div>
      </form.AppForm>
    </form>
  );
}
