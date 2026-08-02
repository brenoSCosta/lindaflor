import { ORPCError } from "@orpc/client";
import { useQuery } from "@tanstack/react-query";
import { format } from "date-fns";
import { Mail, X } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import {
  Empty,
  EmptyDescription,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { useAppAbility } from "@/lib/ability";
import { authClient } from "@/lib/auth-client";
import { InviteMemberDialog } from "@/routes/(auth)/permissions/-components/invite-member-dialog";

const RoleBadge = ({ role }: { role: string }) => {
  if (role === "owner") return <Badge>Proprietário</Badge>;
  if (role === "admin") return <Badge variant="secondary">Administrador</Badge>;
  return <Badge variant="outline">Membro</Badge>;
};

function CancelInvitationButton({
  invitationId,
  email,
  onCancelled,
}: {
  invitationId: string;
  email: string;
  onCancelled: () => Promise<void> | void;
}) {
  const [pending, setPending] = useState(false);

  const handleCancel = async () => {
    setPending(true);
    await authClient.organization.cancelInvitation(
      { invitationId },
      {
        onSuccess: async () => {
          toast.success(`Convite para ${email} cancelado`);
          await onCancelled();
        },
        onError: (e) => {
          toast.error(e.error.message ?? "Falha ao cancelar o convite");
        },
      },
    );
    setPending(false);
  };

  return (
    <Button
      variant="ghost"
      size="icon"
      className="text-muted-foreground hover:text-destructive"
      disabled={pending}
      onClick={handleCancel}
    >
      <X className="size-4" />
      <span className="sr-only">Cancelar convite para {email}</span>
    </Button>
  );
}

export function InvitationsCard({
  organizationId,
}: {
  organizationId: string;
}) {
  const ability = useAppAbility();
  const canInvite = ability.can("create", "Member");

  const { data, refetch: refetchInvitations } = useQuery({
    queryKey: ["organization", "invitations", organizationId],
    queryFn: async () => {
      const res = await authClient.organization.listInvitations({
        query: { organizationId },
      });
      if (res.error) {
        throw new ORPCError("BAD_GATEWAY", {
          message: res.error.message ?? "Falha ao carregar convites",
        });
      }
      return res.data;
    },
  });

  // Only members who can invite manage invitations.
  if (!canInvite) return null;

  const pending = (data ?? []).filter(
    (invitation) => invitation.status === "pending",
  );

  const refetch = async () => {
    await refetchInvitations();
  };

  return (
    <Card>
      <CardHeader>
        <div className="flex items-center gap-2">
          <Mail className="size-5 text-muted-foreground" />
          <CardTitle>Convites pendentes</CardTitle>
          <div className="ml-auto flex items-center gap-2">
            {pending.length > 0 && (
              <Badge variant="secondary">{pending.length}</Badge>
            )}
            <InviteMemberDialog
              organizationId={organizationId}
              onInvited={refetch}
            />
          </div>
        </div>
        <CardDescription>
          Pessoas que foram convidadas, mas ainda não entraram.
        </CardDescription>
      </CardHeader>
      <CardContent>
        {pending.length === 0 ? (
          <Empty>
            <EmptyMedia variant="icon">
              <Mail className="size-8" />
            </EmptyMedia>
            <EmptyTitle>Nenhum convite pendente</EmptyTitle>
            <EmptyDescription>Convide alguém para começar.</EmptyDescription>
          </Empty>
        ) : (
          <div className="rounded-md border">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>E-mail</TableHead>
                  <TableHead>Papel na org</TableHead>
                  <TableHead>Expira</TableHead>
                  <TableHead className="w-12 text-right" />
                </TableRow>
              </TableHeader>
              <TableBody>
                {pending.map((invitation) => (
                  <TableRow key={invitation.id}>
                    <TableCell className="font-medium">
                      {invitation.email}
                    </TableCell>
                    <TableCell>
                      <RoleBadge role={invitation.role} />
                    </TableCell>
                    <TableCell className="text-muted-foreground">
                      {format(new Date(invitation.expiresAt), "PP")}
                    </TableCell>
                    <TableCell className="text-right">
                      <CancelInvitationButton
                        invitationId={invitation.id}
                        email={invitation.email}
                        onCancelled={refetch}
                      />
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </div>
        )}
      </CardContent>
    </Card>
  );
}
