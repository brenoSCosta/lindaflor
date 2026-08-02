import { getRouteApi, Link, useNavigate } from "@tanstack/react-router";
import { toast } from "sonner";
import { z } from "zod";

import { useAppForm } from "@/components/form/hooks";
import { authClient } from "@/lib/auth-client";

const schema = z
  .object({
    newPassword: z.string().min(8, "Pelo menos 8 caracteres"),
    confirmNewPassword: z.string().min(8, "Pelo menos 8 caracteres"),
  })
  .refine((data) => data.newPassword === data.confirmNewPassword, {
    message: "As senhas não coincidem",
    path: ["confirmNewPassword"],
  });

const route = getRouteApi("/reset-password");

export function ResetPassword() {
  const navigate = useNavigate();
  const { token } = route.useSearch();

  const form = useAppForm({
    defaultValues: { newPassword: "", confirmNewPassword: "" },
    onSubmit: async ({ value }) => {
      await authClient.resetPassword(
        { newPassword: value.newPassword, token },
        {
          onSuccess: () => {
            toast.success("Senha redefinida. Faça login.");
            void navigate({ to: "/login" });
          },
          onError: (error) => {
            toast.error(error.error.message || error.error.statusText);
          },
        },
      );
    },
    validators: {
      onChange: schema,
    },
    canSubmitWhenInvalid: true,
  });

  return (
    <div className="mx-auto w-full mt-10 max-w-md p-6">
      <h1 className="mb-2 text-center text-3xl font-bold">Redefinir senha</h1>
      <p className="mb-6 text-center text-sm text-muted-foreground">
        Escolha uma nova senha para sua conta.
      </p>

      <form
        action={async () => {
          await form.handleSubmit();
        }}
        className="space-y-4"
      >
        <div>
          <form.AppField name="newPassword">
            {(field) => (
              <div className="space-y-2">
                <field.Label>Nova senha</field.Label>
                <field.Input type="password" />
                <field.Error />
              </div>
            )}
          </form.AppField>
        </div>

        <div>
          <form.AppField name="confirmNewPassword">
            {(field) => (
              <div className="space-y-2">
                <field.Label>Confirmar nova senha</field.Label>
                <field.Input type="password" />
                <field.Error />
              </div>
            )}
          </form.AppField>
        </div>

        <form.AppForm>
          <form.Button
            className="w-full"
            disabled={form.state.isSubmitting}
            loading={form.state.isSubmitting}
            loadingText="Atualizando..."
          >
            Redefinir senha
          </form.Button>
        </form.AppForm>
      </form>

      <div className="mt-4 text-center">
        <Link
          to="/login"
          className="text-sm text-indigo-600 hover:text-indigo-800"
        >
          Voltar para o login
        </Link>
      </div>
    </div>
  );
}
