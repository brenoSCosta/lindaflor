import { useMutation } from "@tanstack/react-query";
import { toast } from "sonner";
import { z } from "zod";

import { useAppForm } from "@/components/form/hooks";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { orpc } from "@/lib/orpc";
import { todayIsoDate } from "@/lib/utils";
import { CertificateDateField } from "@/routes/(auth)/arqueacao/-components/certificate-date-field";
import { invalidateTankCalibrations } from "@/routes/(auth)/arqueacao/-components/invalidate-tank-calibrations";

export function CreateCertificateForm({
  tankId,
  onCreated,
  onCancel,
}: {
  tankId: string;
  onCreated: (id: string) => void;
  onCancel: () => void;
}) {
  const today = todayIsoDate();

  const createMutation = useMutation(
    orpc.tanks.v1.calibration.create.mutationOptions({
      onSuccess: async (created) => {
        toast.success("Certificado de arqueação criado");
        await invalidateTankCalibrations(tankId, created.id);
        onCreated(created.id);
      },
      onError: (error) => toast.error(error.message),
    }),
  );

  const createForm = useAppForm({
    defaultValues: {
      certificate_number: "",
      issued_at: today,
      valid_from: today,
      valid_until: "",
    },
    validators: {
      onSubmit: z
        .object({
          certificate_number: z
            .string()
            .min(1, { message: "Informe o número do certificado" }),
          issued_at: z.union([z.iso.date(), z.literal("")]),
          valid_from: z.iso.date(),
          valid_until: z.union([z.iso.date(), z.literal("")]),
        })
        .refine(
          (value) =>
            value.valid_until === "" || value.valid_until >= value.valid_from,
          {
            message:
              "Validade final deve ser igual ou posterior ao início da vigência",
            path: ["valid_until"],
          },
        ),
    },
    onSubmit: ({ value }) => {
      createMutation.mutate({
        tank_id: tankId,
        certificate_number: value.certificate_number,
        issued_at: value.issued_at === "" ? null : value.issued_at,
        valid_from: value.valid_from,
        valid_until: value.valid_until === "" ? null : value.valid_until,
        points: [],
      });
    },
  });

  return (
    <Card className="overflow-auto">
      <CardHeader>
        <CardTitle>Novo certificado</CardTitle>
        <CardDescription>
          Informe os metadados do certificado de arqueação.
        </CardDescription>
      </CardHeader>
      <CardContent>
        <form
          className="space-y-4"
          onSubmit={(e) => {
            e.preventDefault();
            void createForm.handleSubmit();
          }}
        >
          <createForm.AppForm>
            <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
              <createForm.AppField name="certificate_number">
                {(field) => (
                  <field.Field>
                    <field.Label className="text-xs font-semibold tracking-wider uppercase">
                      Número do certificado
                    </field.Label>
                    <field.Input />
                    <field.Error />
                  </field.Field>
                )}
              </createForm.AppField>
              <createForm.AppField name="issued_at">
                {(field) => (
                  <field.Field>
                    <field.Label className="text-xs font-semibold tracking-wider uppercase">
                      Emissão
                    </field.Label>
                    <CertificateDateField
                      id={field.name}
                      value={field.state.value}
                      onChange={field.handleChange}
                      onBlur={field.handleBlur}
                      placeholder="Opcional"
                    />
                    <field.Error />
                  </field.Field>
                )}
              </createForm.AppField>
              <createForm.AppField name="valid_from">
                {(field) => (
                  <field.Field>
                    <field.Label className="text-xs font-semibold tracking-wider uppercase">
                      Data início
                    </field.Label>
                    <CertificateDateField
                      id={field.name}
                      value={field.state.value}
                      onChange={field.handleChange}
                      onBlur={field.handleBlur}
                    />
                    <field.Error />
                  </field.Field>
                )}
              </createForm.AppField>
              <createForm.AppField name="valid_until">
                {(field) => (
                  <field.Field>
                    <field.Label className="text-xs font-semibold tracking-wider uppercase">
                      Data validade
                    </field.Label>
                    <CertificateDateField
                      id={field.name}
                      value={field.state.value}
                      onChange={field.handleChange}
                      onBlur={field.handleBlur}
                      placeholder="Opcional"
                    />
                    <field.Error />
                  </field.Field>
                )}
              </createForm.AppField>
            </div>
          </createForm.AppForm>
          <div className="flex justify-end gap-2">
            <Button type="button" variant="outline" onClick={onCancel}>
              Cancelar
            </Button>
            <Button type="submit" disabled={createMutation.isPending}>
              {createMutation.isPending ? "Criando…" : "Criar"}
            </Button>
          </div>
        </form>
      </CardContent>
    </Card>
  );
}
