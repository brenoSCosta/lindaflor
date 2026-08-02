import { ORPCError } from "@orpc/client";
import { useMutation } from "@tanstack/react-query";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { authClient } from "@/lib/auth-client";

type SocialProvider = "google";

const providers: { id: SocialProvider; label: string }[] = [
  { id: "google", label: "Continuar com Google" },
];

const EMAIL_MISMATCH_MESSAGE =
  "O e-mail desta conta social não corresponde à sua conta atual. Faça login com a conta social que corresponde ao seu e-mail.";

export function SocialSignInButtons({
  callbackURL = "/dashboard",
  variant = "default",
}: {
  callbackURL?: string;
  variant?: "default" | "store";
}) {
  // oxlint-disable-next-line react-doctor/query-mutation-missing-invalidation
  const mutation = useMutation({
    mutationFn: async (provider: SocialProvider) => {
      return await authClient.signIn.social({
        provider,
        callbackURL: new URL(callbackURL, window.location.origin).toString(),
        fetchOptions: {
          onError: (e) => {
            if (e.error.code === "email_doesn't_match") {
              throw new ORPCError("ERROR", {
                message: EMAIL_MISMATCH_MESSAGE,
                cause: e.error.message ?? e.error.statusText,
              });
            }
            throw new ORPCError("ERROR", {
              message:
                e.error.message ?? e.error.statusText ?? "Falha no login",
            });
          },
        },
      });
    },
    onSuccess: async () => {
      toast.success("Redirecionando para o provedor…");
    },
    onError: (error) => {
      toast.error(error.message);
    },
  });

  return (
    <div className="space-y-2">
      {providers.map((p) => (
        <Button
          key={p.id}
          type="button"
          variant="outline"
          className={
            variant === "store"
              ? "lf-btn-outline h-12 bg-white hover:bg-white"
              : "w-full"
          }
          disabled={mutation.isPending}
          onClick={() => mutation.mutate(p.id)}
        >
          {mutation.isPending && mutation.variables === p.id
            ? "Redirecionando…"
            : p.label}
        </Button>
      ))}
    </div>
  );
}
