import { ORPCError } from "@orpc/client";
import { useMutation } from "@tanstack/react-query";
import { getRouteApi, Link } from "@tanstack/react-router";
import { useState } from "react";
import { toast } from "sonner";
import { useInterval } from "usehooks-ts";

import { Button } from "@/components/ui/button";
import { authClient } from "@/lib/auth-client";

const RESEND_COOLDOWN_SECONDS = 30;

const route = getRouteApi("/check-email");

export function CheckEmail() {
  const { email } = route.useSearch();
  const [cooldown, setCooldown] = useState(0);

  useInterval(() => setCooldown((c) => c - 1), cooldown > 0 ? 1000 : null);

  // oxlint-disable-next-line react-doctor/query-mutation-missing-invalidation
  const resendMutation = useMutation({
    mutationFn: async () => {
      return await authClient.sendVerificationEmail({
        email,
        fetchOptions: {
          onError: (e) => {
            throw new ORPCError("ERROR", {
              message:
                e.error.message ??
                e.error.statusText ??
                "Falha ao enviar e-mail",
            });
          },
        },
      });
    },
    onSuccess: async () => {
      toast.success("E-mail de verificação enviado");
      setCooldown(RESEND_COOLDOWN_SECONDS);
    },
    onError: (error) => {
      toast.error(error.message);
    },
  });

  const resendDisabled = resendMutation.isPending || cooldown > 0;

  return (
    <div className="mx-auto w-full mt-10 max-w-md p-6">
      <h1 className="mb-2 text-center text-3xl font-bold">
        Verifique sua caixa de entrada
      </h1>
      <p className="mb-6 text-center text-sm text-muted-foreground">
        Enviamos um link de verificação para <strong>{email}</strong>. Clique no
        link para concluir o login.
      </p>

      <Button
        type="button"
        className="w-full"
        variant="outline"
        disabled={resendDisabled}
        onClick={() => resendMutation.mutate()}
      >
        {resendMutation.isPending
          ? "Enviando…"
          : cooldown > 0
            ? `Reenviar em ${cooldown}s`
            : "Reenviar e-mail de verificação"}
      </Button>

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
