import type { User } from "@lindaflor/db/schema/auth";
import { getRouteApi, useNavigate } from "@tanstack/react-router";
import React from "react";
import { toast } from "sonner";
import { z } from "zod";

import {
  AuthDivider,
  AuthLayout,
  AuthLink,
} from "@/components/store/auth-layout";
import { useAppForm } from "@/components/form/hooks";
import { Loader } from "@/components/loader";
import { SocialSignInButtons } from "@/components/social-sign-in-buttons";
import { authClient } from "@/lib/auth-client";
import { cn } from "@/lib/utils";

const route = getRouteApi("/login");

const inputClass =
  "lf-input-shadcn h-auto rounded-none border-0 border-b border-[var(--lf-line)] bg-transparent px-0 shadow-none focus-visible:border-[var(--lf-pink)] focus-visible:ring-0";

function SignInForm({
  onSwitchToSignUp,
  defaultEmail,
  inviteId,
}: {
  onSwitchToSignUp: () => void;
  defaultEmail: string;
  inviteId?: string;
}) {
  const navigate = useNavigate();
  const { isPending } = authClient.useSession();
  const { refetch: refetchActiveOrg } = authClient.useActiveOrganization();
  const { refetch: refetchListOrgs } = authClient.useListOrganizations();

  const form = useAppForm({
    defaultValues: {
      email: defaultEmail,
      password: "",
    } satisfies Pick<User, "email"> & { password: string },
    onSubmit: async ({ value }) => {
      await authClient.signIn.email(
        { email: value.email, password: value.password },
        {
          onSuccess: async (ctx) => {
            if (
              ctx.data &&
              "twoFactorRedirect" in ctx.data &&
              ctx.data.twoFactorRedirect
            ) {
              return;
            }
            await Promise.all([refetchActiveOrg(), refetchListOrgs()]);
            toast.success("Login realizado com sucesso");
            if (inviteId) {
              await navigate({
                to: "/accept-invitation",
                search: { id: inviteId, email: value.email },
              });
              return;
            }
            await navigate({ to: "/dashboard" });
          },
          onError: async (error) => {
            if (error.error.code === "EMAIL_NOT_VERIFIED") {
              await navigate({
                to: "/check-email",
                search: { email: value.email },
              });
              return;
            }
            toast.error(error.error.message || error.error.statusText);
          },
        },
      );
    },
    validators: {
      onChange: z.object({
        email: z.email("E-mail inválido"),
        password: z.string().min(8, "Senha deve ter pelo menos 8 caracteres"),
      }),
    },
    canSubmitWhenInvalid: true,
  });

  if (isPending) {
    return <Loader />;
  }

  return (
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

      <form.AppField name="password">
        {(field) => (
          <div className="space-y-2">
            <field.Label className="lf-label">Senha</field.Label>
            <field.PasswordInput
              autoComplete="current-password"
              className={cn(inputClass, "pr-10")}
            />
            <field.Error className="text-xs text-red-600" />
            <div className="flex justify-end pt-1">
              <AuthLink to="/forgot-password">Esqueceu a senha?</AuthLink>
            </div>
          </div>
        )}
      </form.AppField>

      <form.AppForm>
        <form.Button
          className="lf-btn-primary"
          disabled={form.state.isSubmitting}
          loading={form.state.isSubmitting}
          loadingText="Entrando..."
        >
          Entrar
        </form.Button>
      </form.AppForm>

      <p className="text-center text-sm text-[var(--lf-muted)]">
        Precisa de uma conta?{" "}
        <button
          type="button"
          onClick={onSwitchToSignUp}
          className="text-[var(--lf-pink)] transition-colors hover:text-[var(--lf-pink-deep)]"
        >
          Cadastre-se
        </button>
      </p>

      <AuthDivider />
      <SocialSignInButtons variant="store" />
    </form>
  );
}

function SignUpForm({
  onSwitchToSignIn,
  defaultEmail,
  inviteId,
}: {
  onSwitchToSignIn: () => void;
  defaultEmail: string;
  inviteId?: string;
}) {
  const navigate = useNavigate();
  const { isPending } = authClient.useSession();

  const form = useAppForm({
    defaultValues: {
      email: defaultEmail,
      password: "",
      name: "",
    } satisfies Pick<User, "email" | "name"> & { password: string },
    onSubmit: async ({ value }) => {
      const callbackURL = inviteId
        ? `${window.location.origin}/accept-invitation?id=${encodeURIComponent(inviteId)}&email=${encodeURIComponent(value.email)}`
        : undefined;

      await authClient.signUp.email(
        {
          email: value.email,
          password: value.password,
          name: value.name,
          callbackURL,
        },
        {
          onSuccess: async () => {
            void navigate({
              to: "/check-email",
              search: { email: value.email },
            });
          },
          onError: (error) => {
            toast.error(error.error.message || error.error.statusText);
          },
        },
      );
    },
    validators: {
      onChange: z.object({
        name: z.string().min(2, "O nome deve ter pelo menos 2 caracteres"),
        email: z.email("Endereço de e-mail inválido"),
        password: z.string().min(8, "A senha deve ter pelo menos 8 caracteres"),
      }),
    },
    canSubmitWhenInvalid: true,
  });

  if (isPending) {
    return <Loader />;
  }

  return (
    <form
      action={async () => {
        await form.handleSubmit();
      }}
      className="space-y-6"
    >
      <form.AppField name="name">
        {(field) => (
          <div className="space-y-2">
            <field.Label className="lf-label">Nome</field.Label>
            <field.Input className={inputClass} />
            <field.Error className="text-xs text-red-600" />
          </div>
        )}
      </form.AppField>

      <form.AppField name="email">
        {(field) => (
          <div className="space-y-2">
            <field.Label className="lf-label">E-mail</field.Label>
            <field.Input type="email" className={inputClass} />
            <field.Error className="text-xs text-red-600" />
          </div>
        )}
      </form.AppField>

      <form.AppField name="password">
        {(field) => (
          <div className="space-y-2">
            <field.Label className="lf-label">Senha</field.Label>
            <field.PasswordInput
              autoComplete="new-password"
              className={cn(inputClass, "pr-10")}
            />
            <field.Error className="text-xs text-red-600" />
          </div>
        )}
      </form.AppField>

      <form.AppForm>
        <form.Button
          className="lf-btn-primary"
          disabled={form.state.isSubmitting}
          loading={form.state.isSubmitting}
          loadingText="Cadastrando..."
        >
          Cadastrar
        </form.Button>
      </form.AppForm>

      <p className="text-center text-sm text-[var(--lf-muted)]">
        Já tem uma conta?{" "}
        <button
          type="button"
          onClick={onSwitchToSignIn}
          className="text-[var(--lf-pink)] transition-colors hover:text-[var(--lf-pink-deep)]"
        >
          Entrar
        </button>
      </p>

      <AuthDivider />
      <SocialSignInButtons variant="store" />
    </form>
  );
}

export function Login() {
  const { inviteId, email, mode } = route.useSearch();
  const [showSignIn, setShowSignIn] = React.useState(mode !== "signup");
  const defaultEmail = email ?? "";

  return (
    <AuthLayout
      title={showSignIn ? "Bem-vinda de volta" : "Criar conta"}
      subtitle={
        showSignIn
          ? "Entre para acompanhar seus pedidos e favoritos."
          : "Cadastre-se para uma experiência personalizada."
      }
    >
      <React.Activity mode={showSignIn ? "visible" : "hidden"}>
        <SignInForm
          onSwitchToSignUp={() => setShowSignIn(false)}
          defaultEmail={defaultEmail}
          inviteId={inviteId}
        />
      </React.Activity>

      <React.Activity mode={showSignIn ? "hidden" : "visible"}>
        <SignUpForm
          onSwitchToSignIn={() => setShowSignIn(true)}
          defaultEmail={defaultEmail}
          inviteId={inviteId}
        />
      </React.Activity>
    </AuthLayout>
  );
}
