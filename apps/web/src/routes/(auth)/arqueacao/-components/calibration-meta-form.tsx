import type { TankCalibrationDetail } from "@lindaflor/shared/schemas/tankage/calibrations";
import { useMutation } from "@tanstack/react-query";
import { Trash2 } from "lucide-react";
import { toast } from "sonner";
import { z } from "zod";

import { useAppForm } from "@/components/form/hooks";
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
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { orpc } from "@/lib/orpc";
import { CertificateDateField } from "@/routes/(auth)/arqueacao/-components/certificate-date-field";
import { invalidateTankCalibrations } from "@/routes/(auth)/arqueacao/-components/invalidate-tank-calibrations";

export function CalibrationMetaForm({
  detail,
  canManage,
  canDelete,
  readOnlyHint = false,
  tankId,
  onDeleted,
}: {
  detail: TankCalibrationDetail;
  canManage: boolean;
  canDelete: boolean;
  readOnlyHint?: boolean;
  tankId: string;
  onDeleted?: () => void;
}) {
  const updateMutation = useMutation(
    orpc.tanks.v1.calibration.update.mutationOptions({
      onSuccess: async () => {
        toast.success("Certificado atualizado");
        await invalidateTankCalibrations(tankId, detail.id);
      },
      onError: (error) => toast.error(error.message),
    }),
  );

  const deleteMutation = useMutation(
    orpc.tanks.v1.calibration.delete.mutationOptions({
      onSuccess: async () => {
        toast.success("Certificado excluído");
        await invalidateTankCalibrations(tankId, null);
        onDeleted?.();
      },
      onError: (error) => toast.error(error.message),
    }),
  );

  const metaForm = useAppForm({
    defaultValues: {
      certificate_number: detail.certificate_number,
      issued_at: detail.issued_at ?? "",
      valid_from: detail.valid_from,
      valid_until: detail.valid_until ?? "",
    },
    validators: {
      onSubmit: z
        .object({
          certificate_number: z.string().min(1),
          issued_at: z.iso.date().or(z.literal("")),
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
      updateMutation.mutate({
        id: detail.id,
        certificate_number: value.certificate_number,
        issued_at: value.issued_at === "" ? null : value.issued_at,
        valid_from: value.valid_from,
        valid_until: value.valid_until === "" ? null : value.valid_until,
      });
    },
  });

  return (
    <Card className="shrink-0">
      <CardHeader className="pb-0">
        <CardTitle className="text-xs font-semibold tracking-wide text-muted-foreground uppercase">
          Detalhes do certificado
        </CardTitle>
      </CardHeader>
      <CardContent>
        {readOnlyHint ? (
          <p className="mb-3 text-sm text-muted-foreground">
            Certificado expirado. Somente leitura.
          </p>
        ) : null}
        <form
          className="space-y-4"
          onSubmit={(e) => {
            e.preventDefault();
            void metaForm.handleSubmit();
          }}
        >
          <metaForm.AppForm>
            <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
              <metaForm.AppField name="certificate_number">
                {(field) => (
                  <field.Field>
                    <field.Label className="text-xs font-semibold tracking-wider uppercase">
                      Número do certificado
                    </field.Label>
                    <field.Input disabled={!canManage} />
                    <field.Error />
                  </field.Field>
                )}
              </metaForm.AppField>
              <metaForm.AppField name="issued_at">
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
                      disabled={!canManage}
                      placeholder="Opcional"
                    />
                    <field.Error />
                  </field.Field>
                )}
              </metaForm.AppField>
              <metaForm.AppField name="valid_from">
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
                      disabled={!canManage}
                    />
                    <field.Error />
                  </field.Field>
                )}
              </metaForm.AppField>
              <metaForm.AppField name="valid_until">
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
                      disabled={!canManage}
                      placeholder="Opcional"
                    />
                    <field.Error />
                  </field.Field>
                )}
              </metaForm.AppField>
            </div>
          </metaForm.AppForm>
          <div className="flex flex-wrap items-center justify-between gap-2">
            {canDelete ? (
              <AlertDialog>
                <AlertDialogTrigger
                  render={(props) => (
                    <Button
                      type="button"
                      variant="ghost"
                      size="sm"
                      className="text-destructive"
                      disabled={deleteMutation.isPending}
                      {...props}
                    >
                      <Trash2 className="size-4" />
                      Excluir
                    </Button>
                  )}
                />
                <AlertDialogContent>
                  <AlertDialogHeader>
                    <AlertDialogTitle>
                      Excluir certificado &quot;{detail.certificate_number}
                      &quot;?
                    </AlertDialogTitle>
                    <AlertDialogDescription>
                      Esta ação remove o certificado e toda a tabela altura →
                      volume. Não pode ser desfeita.
                    </AlertDialogDescription>
                  </AlertDialogHeader>
                  <AlertDialogFooter>
                    <AlertDialogCancel disabled={deleteMutation.isPending}>
                      Cancelar
                    </AlertDialogCancel>
                    <AlertDialogAction
                      variant="destructive"
                      disabled={deleteMutation.isPending}
                      onClick={() =>
                        deleteMutation.mutate({ ids: [detail.id] })
                      }
                    >
                      {deleteMutation.isPending ? "Excluindo…" : "Excluir"}
                    </AlertDialogAction>
                  </AlertDialogFooter>
                </AlertDialogContent>
              </AlertDialog>
            ) : null}
            {canManage ? (
              <Button
                type="submit"
                size="sm"
                className="ml-auto"
                disabled={updateMutation.isPending}
              >
                {updateMutation.isPending ? "Salvando…" : "Salvar metadados"}
              </Button>
            ) : null}
          </div>
        </form>
      </CardContent>
    </Card>
  );
}
