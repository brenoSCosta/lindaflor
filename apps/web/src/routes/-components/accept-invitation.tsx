import { useQuery } from "@tanstack/react-query";
import { getRouteApi, useNavigate } from "@tanstack/react-router";
import { Building2, Check, X } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";

import { Loader } from "@/components/loader";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { authClient } from "@/lib/auth-client";

const route = getRouteApi("/accept-invitation");

const ROLE_LABELS: Record<string, string> = {
  owner: "Proprietário",
  admin: "Administrador",
  member: "Membro",
};

const roleLabel = (role: string): string => ROLE_LABELS[role] ?? role;

function InvitePageShell({ children }: { children: React.ReactNode }) {
  return (
    <div className="mx-auto w-full mt-10 max-w-md p-6">
      <Card>{children}</Card>
    </div>
  );
}

function UnauthenticatedInvite({ id, email }: { id: string; email?: string }) {
  const navigate = useNavigate();

  return (
    <InvitePageShell>
      <CardHeader>
        <div className="flex items-center gap-2">
          <Building2 className="size-5 text-muted-foreground" />
          <CardTitle>Você foi convidado</CardTitle>
        </div>
        <CardDescription>
          Faça login{email ? ` como ${email}` : ""} ou crie uma conta para
          aceitar este convite.
        </CardDescription>
      </CardHeader>
      <CardFooter className="flex flex-col gap-2">
        <Button
          className="w-full"
          onClick={() =>
            navigate({
              to: "/login",
              search: { inviteId: id, email, mode: "signin" },
            })
          }
        >
          Entrar
        </Button>
        <Button
          variant="outline"
          className="w-full"
          onClick={() =>
            navigate({
              to: "/login",
              search: { inviteId: id, email, mode: "signup" },
            })
          }
        >
          Criar conta
        </Button>
      </CardFooter>
    </InvitePageShell>
  );
}

function InvitationError({
  id,
  email,
  message,
}: {
  id: string;
  email?: string;
  message: string;
}) {
  const navigate = useNavigate();
  const [signingOut, setSigningOut] = useState(false);

  const handleSignOut = async () => {
    setSigningOut(true);
    await authClient.signOut();
    void navigate({
      to: "/login",
      search: { inviteId: id, email, mode: "signin" },
    });
  };

  return (
    <InvitePageShell>
      <CardHeader>
        <CardTitle>Convite indisponível</CardTitle>
        <CardDescription>{message}</CardDescription>
      </CardHeader>
      <CardFooter className="flex flex-col gap-2">
        <Button
          variant="outline"
          className="w-full"
          disabled={signingOut}
          onClick={handleSignOut}
        >
          Entrar com outra conta
        </Button>
        <Button
          variant="ghost"
          className="w-full"
          onClick={() => navigate({ to: "/dashboard" })}
        >
          Ir para o painel
        </Button>
      </CardFooter>
    </InvitePageShell>
  );
}

function InvitationDetails({ id, email }: { id: string; email?: string }) {
  const navigate = useNavigate();
  const [actionPending, setActionPending] = useState(false);

  const { data, isPending, isError, error } = useQuery({
    queryKey: ["organization", "invitation", id],
    queryFn: async () => {
      const res = await authClient.organization.getInvitation({
        query: { id },
      });
      if (res.error) {
        throw new Error(res.error.message ?? "Este convite não é mais válido.");
      }
      return res.data;
    },
    retry: false,
  });

  if (isPending) {
    return <Loader />;
  }

  if (isError) {
    return <InvitationError id={id} email={email} message={error.message} />;
  }

  const invitation = data;

  const handleAccept = async () => {
    setActionPending(true);
    await authClient.organization.acceptInvitation(
      { invitationId: id },
      {
        onSuccess: async () => {
          await authClient.organization.setActive({
            organizationId: data.organizationId,
          });
          toast.success(`Você entrou em ${data.organizationName}`);
          void navigate({ to: "/permissions" });
        },
        onError: (e) => {
          toast.error(e.error.message ?? "Falha ao aceitar o convite");
        },
      },
    );
    setActionPending(false);
  };

  const handleDecline = async () => {
    setActionPending(true);
    await authClient.organization.rejectInvitation(
      { invitationId: id },
      {
        onSuccess: () => {
          toast.success("Convite recusado");
          void navigate({ to: "/" });
        },
        onError: (e) => {
          toast.error(e.error.message ?? "Falha ao recusar o convite");
        },
      },
    );
    setActionPending(false);
  };

  return (
    <InvitePageShell>
      <CardHeader>
        <div className="flex items-center gap-2">
          <Building2 className="size-5 text-muted-foreground" />
          <CardTitle>{invitation.organizationName}</CardTitle>
        </div>
        <CardDescription>
          {invitation.inviterEmail} convidou você para entrar como{" "}
          {roleLabel(invitation.role)}.
        </CardDescription>
      </CardHeader>
      <CardContent>
        <p className="text-sm text-muted-foreground">
          Aceitar adicionará você a {invitation.organizationName} e a tornará
          sua organização ativa.
        </p>
      </CardContent>
      <CardFooter className="flex gap-2">
        <Button
          className="flex-1"
          disabled={actionPending}
          onClick={handleAccept}
        >
          <Check className="size-4" />
          Aceitar
        </Button>
        <Button
          variant="outline"
          className="flex-1"
          disabled={actionPending}
          onClick={handleDecline}
        >
          <X className="size-4" />
          Recusar
        </Button>
      </CardFooter>
    </InvitePageShell>
  );
}

export function AcceptInvitation() {
  const { id, email } = route.useSearch();
  const { data: session, isPending } = authClient.useSession();

  if (isPending) {
    return <Loader />;
  }

  if (!session) {
    return <UnauthenticatedInvite id={id} email={email} />;
  }

  return <InvitationDetails id={id} email={email} />;
}
