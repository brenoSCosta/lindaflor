import { toast } from "sonner";
import { z } from "zod";

import { useAppForm } from "@/components/form/hooks";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Separator } from "@/components/ui/separator";
import { Skeleton } from "@/components/ui/skeleton";
import { authClient } from "@/lib/auth-client";

const emailSchema = z.object({
  newEmail: z.email("Endereço de e-mail inválido"),
});

const passwordSchema = z
  .object({
    currentPassword: z.string().min(1, "Obrigatório"),
    newPassword: z.string().min(8, "Pelo menos 8 caracteres"),
    confirmNewPassword: z.string().min(8, "Pelo menos 8 caracteres"),
  })
  .refine((data) => data.newPassword === data.confirmNewPassword, {
    message: "As senhas não coincidem",
    path: ["confirmNewPassword"],
  });

export function AccountCard() {
  const { data: session, isPending } = authClient.useSession();

  const emailForm = useAppForm({
    defaultValues: { newEmail: "" },
    validators: { onChange: emailSchema },
    onSubmit: async ({ value, formApi }) => {
      await authClient.changeEmail(
        {
          newEmail: value.newEmail,
          callbackURL: "/settings?tab=account",
        },
        {
          onSuccess: () => {
            toast.success(
              "Verifique sua caixa de entrada antiga para confirmar a alteração.",
            );
            formApi.reset();
          },
          onError: (e) => {
            toast.error(e.error.message ?? e.error.statusText);
          },
        },
      );
    },
  });

  const passwordForm = useAppForm({
    defaultValues: {
      currentPassword: "",
      newPassword: "",
      confirmNewPassword: "",
    },
    validators: { onChange: passwordSchema },
    onSubmit: async ({ value, formApi }) => {
      await authClient.changePassword(
        {
          currentPassword: value.currentPassword,
          newPassword: value.newPassword,
          revokeOtherSessions: true,
        },
        {
          onSuccess: () => {
            toast.success("Senha atualizada. Outras sessões encerradas.");
            formApi.reset();
          },
          onError: (e) => {
            toast.error(e.error.message ?? e.error.statusText);
          },
        },
      );
    },
  });

  if (isPending || !session) {
    return (
      <Card>
        <CardHeader>
          <CardTitle>Conta</CardTitle>
          <CardDescription>Atualize seu e-mail ou senha.</CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <Skeleton className="h-10 w-full" />
          <Skeleton className="h-10 w-full" />
        </CardContent>
      </Card>
    );
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>Conta</CardTitle>
        <CardDescription>
          E-mail atual:{" "}
          <span className="font-medium">{session.user.email}</span>
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-6">
        <form
          action={async () => {
            await emailForm.handleSubmit();
          }}
          className="space-y-4"
        >
          <h3 className="text-sm font-medium">Alterar e-mail</h3>
          <p className="text-xs text-muted-foreground">
            Enviaremos um link de confirmação para seu e-mail atual. A alteração
            entra em vigor assim que você clicar nele.
          </p>
          <emailForm.AppForm>
            <emailForm.AppField name="newEmail">
              {(field) => (
                <field.Field>
                  <field.Label>Novo e-mail</field.Label>
                  <field.Input type="email" placeholder="voce@exemplo.com" />
                  <field.Error />
                </field.Field>
              )}
            </emailForm.AppField>
            <emailForm.Button
              disabled={emailForm.state.isSubmitting}
              loading={emailForm.state.isSubmitting}
              loadingText="Atualizando..."
            >
              Atualizar e-mail
            </emailForm.Button>
          </emailForm.AppForm>
        </form>

        <Separator />

        <form
          action={async () => {
            await passwordForm.handleSubmit();
          }}
          className="space-y-4"
        >
          <h3 className="text-sm font-medium">Alterar senha</h3>
          <passwordForm.AppForm>
            <div className="grid gap-4">
              <passwordForm.AppField name="currentPassword">
                {(field) => (
                  <field.Field>
                    <field.Label>Senha atual</field.Label>
                    <field.Input
                      type="password"
                      autoComplete="current-password"
                    />
                    <field.Error />
                  </field.Field>
                )}
              </passwordForm.AppField>
              <passwordForm.AppField name="newPassword">
                {(field) => (
                  <field.Field>
                    <field.Label>Nova senha</field.Label>
                    <field.Input type="password" autoComplete="new-password" />
                    <field.Error />
                  </field.Field>
                )}
              </passwordForm.AppField>
              <passwordForm.AppField name="confirmNewPassword">
                {(field) => (
                  <field.Field>
                    <field.Label>Confirmar nova senha</field.Label>
                    <field.Input type="password" autoComplete="new-password" />
                    <field.Error />
                  </field.Field>
                )}
              </passwordForm.AppField>
            </div>
            <passwordForm.Button
              disabled={passwordForm.state.isSubmitting}
              loading={passwordForm.state.isSubmitting}
              loadingText="Atualizando..."
            >
              Atualizar senha
            </passwordForm.Button>
          </passwordForm.AppForm>
        </form>
      </CardContent>
    </Card>
  );
}
