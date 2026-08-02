import {
  zonedDateTimeToUtc,
  zonedParts,
} from "@lindaflor/shared/lib/zoned-datetime";
import { schema } from "@lindaflor/shared/schemas/tankage/transfers";
import type { TankTransferOutput } from "@lindaflor/shared/schemas/tankage/transfers";
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

const heightSchema = z.number().min(0, {
  message: "Informe a altura",
});

const retreatSchema = schema.retreat.input;

type RetreatValues = {
  transferred_at: Date;
  height_before_m: number;
  height_after_m: number;
  oil_temperature_c: number;
  ambient_temperature_c: number;
  destination_label: string;
  observation: string;
  justification: string;
};

interface TankTransferRetreatFormProps {
  transfer: TankTransferOutput;
  day: string;
  formId?: string;
  onSubmitted?: () => void;
}

export function TankTransferRetreatForm({
  transfer,
  day,
  formId = "tank-transfer-retreat-form",
  onSubmitted,
}: TankTransferRetreatFormProps) {
  const { timezone } = useTimezone();
  const previousTimezoneRef = React.useRef(timezone);

  const mutation = useMutation(
    orpc.tanks.v1.transfer.retreat.mutationOptions({
      onError: (error) => toast.error(error.message),
    }),
  );

  const form = useAppForm({
    defaultValues: {
      transferred_at: transfer.transferred_at,
      height_before_m: transfer.height_before_m,
      height_after_m: transfer.height_after_m,
      oil_temperature_c: transfer.oil_temperature_c,
      ambient_temperature_c: transfer.ambient_temperature_c,
      destination_label: transfer.destination_label ?? "",
      observation: transfer.observation,
      justification: "",
    } satisfies RetreatValues,
    onSubmit: async ({ value }) => {
      const nextDestination =
        value.destination_label.trim() === ""
          ? null
          : value.destination_label.trim();
      const payload: z.infer<typeof retreatSchema> = {
        id: transfer.id,
        justification: value.justification.trim(),
      };
      if (
        value.transferred_at.getTime() !== transfer.transferred_at.getTime()
      ) {
        payload.transferred_at = value.transferred_at;
      }
      if (value.height_before_m !== transfer.height_before_m) {
        payload.height_before_m = value.height_before_m;
      }
      if (value.height_after_m !== transfer.height_after_m) {
        payload.height_after_m = value.height_after_m;
      }
      if (value.oil_temperature_c !== transfer.oil_temperature_c) {
        payload.oil_temperature_c = value.oil_temperature_c;
      }
      if (value.ambient_temperature_c !== transfer.ambient_temperature_c) {
        payload.ambient_temperature_c = value.ambient_temperature_c;
      }
      if (nextDestination !== transfer.destination_label) {
        payload.destination_label = nextDestination;
      }
      if (value.observation.trim() !== transfer.observation) {
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
      await invalidateTankDay({
        tankId: transfer.tank_id,
        operationalDay: day,
      });
      onSubmitted?.();
    },
  });

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

        <form.AppField
          name="justification"
          validators={{
            onChange: z
              .string()
              .min(1, { message: "Informe a justificativa do retratamento" }),
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
