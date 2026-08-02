import { AuthLayout, AuthLink } from "@/components/store/auth-layout";
import { useAppForm } from "@/components/form/hooks";
import { authClient } from "@/lib/auth-client";
import { toast } from "sonner";
import { z } from "zod";

const inputClass =
  "lf-input-shadcn h-auto rounded-none border-0 border-b border-[var(--lf-line)] bg-transparent px-0 shadow-none focus-visible:border-[var(--lf-pink)] focus-visible:ring-0";

const schema = z.object({
  email: z.email("Endereço de e-mail inválido"),
});

export function ForgotPassword() {
  const form = useAppForm({
    defaultValues: { email: "" },
    onSubmit: async ({ value }) => {
      await authClient.requestPasswordReset(
        {
          email: value.email,
          redirectTo: `${window.location.origin}/reset-password`,
        },
        {
          onSuccess: () => {
            toast.success(
              "Se existir uma conta com esse e-mail, um link de redefinição foi enviado.",
            );
          },
          onError: (error) => {
            toast.error(error.error.message || error.error.statusText);
          },
        },
      );
    },
    validators: { onChange: schema },
    canSubmitWhenInvalid: true,
  });

  return (
    <AuthLayout
      title="Esqueceu a senha"
      subtitle="Informe seu e-mail e enviaremos um link para redefinir sua senha."
    >
      {form.state.isSubmitSuccessful ? (
        <div className="space-y-6 text-center">
          <p className="text-sm text-[var(--lf-muted)]">
            Se existir uma conta com esse e-mail, um link de redefinição foi
            enviado. Verifique sua caixa de entrada.
          </p>
          <AuthLink to="/login">Voltar para o login</AuthLink>
        </div>
      ) : (
        <>
          <form
            action={async () => {
              await form.handleSubmit();
            }}
            className="space-y-6"
          >
            <form.AppField name="email">
              {(field) => (
                <div className="space-y-2">
                  <field.Label className="lf-label">E-mail</field.Label>
                  <field.Input type="email" className={inputClass} />
                  <field.Error className="text-xs text-red-600" />
                </div>
              )}
            </form.AppField>

            <form.AppForm>
              <form.Button
                className="lf-btn-primary"
                disabled={form.state.isSubmitting}
                loading={form.state.isSubmitting}
                loadingText="Enviando..."
              >
                Enviar link
              </form.Button>
            </form.AppForm>
          </form>

          <div className="mt-6 text-center">
            <AuthLink to="/login">Voltar para o login</AuthLink>
          </div>
        </>
      )}
    </AuthLayout>
  );
}
