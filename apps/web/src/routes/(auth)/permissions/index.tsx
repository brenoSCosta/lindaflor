import { createFileRoute } from "@tanstack/react-router";
import { Shield } from "lucide-react";

import {
  Empty,
  EmptyDescription,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty";
import { authClient } from "@/lib/auth-client";
import { InvitationsCard } from "@/routes/(auth)/permissions/-components/invitations-card";
import { MembersCard } from "@/routes/(auth)/permissions/-components/members-card";
import { YourAccessCard } from "@/routes/(auth)/permissions/-components/your-access-card";

export const Route = createFileRoute("/(auth)/permissions/")({
  component: RouteComponent,
});

function RouteComponent() {
  const { data: activeOrganization, isPending } =
    authClient.useActiveOrganization();

  return (
    <div className="container max-w-5xl mx-auto py-8 px-4 space-y-8">
      <div className="space-y-1">
        <h1 className="text-2xl font-semibold tracking-tight">Permissões</h1>
        <p className="text-sm text-muted-foreground">
          Veja o que você pode fazer e gerencie quem tem acesso a{" "}
          {activeOrganization?.name ?? "esta organização"}.
        </p>
      </div>

      <YourAccessCard />

      {isPending ? null : activeOrganization ? (
        <>
          <MembersCard organization={activeOrganization} />
          <InvitationsCard organizationId={activeOrganization.id} />
        </>
      ) : (
        <Empty>
          <EmptyMedia variant="icon">
            <Shield className="size-8" />
          </EmptyMedia>
          <EmptyTitle>Nenhuma organização ativa</EmptyTitle>
          <EmptyDescription>
            Escolha uma organização no cabeçalho para gerenciar seus membros.
          </EmptyDescription>
        </Empty>
      )}
    </div>
  );
}
