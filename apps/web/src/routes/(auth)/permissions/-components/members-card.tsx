import { subject } from "@lindaflor/shared/lib/ability/subjects";
import type { OrgRoles } from "@lindaflor/shared/lib/roles";
import { format } from "date-fns";
import { Trash2, Users } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";

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
import { Avatar, AvatarFallback } from "@/components/ui/avatar";
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
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { Can, useAppAbility } from "@/lib/ability";
import { authClient } from "@/lib/auth-client";

type ActiveOrganization = NonNullable<
  ReturnType<typeof authClient.useActiveOrganization>["data"]
>;

type ActiveMember = ActiveOrganization["members"][number];

const isOrgRole = (r: string): r is OrgRoles =>
  r === "owner" || r === "admin" || r === "operator" || r === "member";

const initialsOf = (name: string | null | undefined) => {
  if (!name) return "?";
  const parts = name.trim().split(/\s+/u);
  const first = parts[0]?.[0] ?? "";
  const second = parts.length > 1 ? (parts.at(-1)?.[0] ?? "") : "";
  return (first + second).toUpperCase() || "?";
};

const RoleBadge = ({ role }: { role: string }) => {
  if (role === "owner") return <Badge>Proprietário</Badge>;
  if (role === "admin") return <Badge variant="secondary">Administrador</Badge>;
  if (role === "operator") return <Badge variant="secondary">Operador</Badge>;
  return <Badge variant="outline">Membro</Badge>;
};

export function MembersCard({
  organization,
}: {
  organization: ActiveOrganization;
}) {
  const { data: session } = authClient.useSession();
  const { refetch } = authClient.useActiveOrganization();
  const ability = useAppAbility();

  const currentUserId = session?.user.id;
  const members = organization.members;

  const canManageAnyMember = members.some((m) =>
    ability.can(
      "update",
      subject("Member", {
        id: m.id,
        organization_id: m.organizationId,
        user_id: m.userId,
        role: m.role,
        created_at: m.createdAt,
      }),
    ),
  );

  return (
    <Card>
      <CardHeader>
        <div className="flex items-center gap-2">
          <Users className="size-5 text-muted-foreground" />
          <CardTitle>Membros da organização</CardTitle>
          <Badge variant="secondary" className="ml-auto">
            {members.length}
          </Badge>
        </div>
        <CardDescription>
          {canManageAnyMember
            ? "Altere papéis ou remova membros. As alterações se aplicam imediatamente."
            : "Estas são as pessoas desta organização."}
        </CardDescription>
      </CardHeader>
      <CardContent>
        <div className="overflow-x-auto rounded-md border">
          <Table className="min-w-160">
            <TableHeader>
              <TableRow>
                <TableHead>Membro</TableHead>
                <TableHead>E-mail</TableHead>
                <TableHead>Papel na org</TableHead>
                <TableHead>Entrou em</TableHead>
                <TableHead className="w-12 text-right" />
              </TableRow>
            </TableHeader>
            <TableBody>
              {members.map((member) => (
                <MemberRow
                  key={member.id}
                  member={member}
                  organizationId={organization.id}
                  isSelf={member.userId === currentUserId}
                  onMutated={async () => {
                    await refetch();
                  }}
                />
              ))}
            </TableBody>
          </Table>
        </div>
      </CardContent>
    </Card>
  );
}

function MemberRow({
  member,
  organizationId,
  isSelf,
  onMutated,
}: {
  member: ActiveMember;
  organizationId: string;
  isSelf: boolean;
  onMutated: () => Promise<void>;
}) {
  const memberSubject = subject("Member", {
    id: member.id,
    organization_id: member.organizationId,
    user_id: member.userId,
    role: member.role,
    created_at: member.createdAt,
  });
  const role = isOrgRole(member.role) ? member.role : "member";
  const name = member.user?.name ?? member.user?.email ?? "Desconhecido";

  return (
    <TableRow>
      <TableCell>
        <div className="flex items-center gap-3">
          <Avatar size="sm">
            <AvatarFallback>{initialsOf(member.user?.name)}</AvatarFallback>
          </Avatar>
          <div className="flex items-center gap-2">
            <span className="font-medium">{name}</span>
            {isSelf && (
              <Badge variant="outline" className="text-xs">
                Você
              </Badge>
            )}
          </div>
        </div>
      </TableCell>
      <TableCell className="text-muted-foreground">
        <span
          className="block max-w-48 truncate"
          title={member.user?.email ?? undefined}
        >
          {member.user?.email ?? "—"}
        </span>
      </TableCell>
      <TableCell>
        <Can I="update" this={memberSubject} passThrough>
          {({ isAllowed }) =>
            isAllowed && !isSelf ? (
              <ChangeRoleSelect
                memberId={member.id}
                currentRole={role}
                organizationId={organizationId}
                onChanged={onMutated}
              />
            ) : (
              <RoleBadge role={role} />
            )
          }
        </Can>
      </TableCell>
      <TableCell className="text-muted-foreground">
        {format(new Date(member.createdAt), "PP")}
      </TableCell>
      <TableCell className="text-right">
        {!isSelf && (
          <Can I="delete" this={memberSubject}>
            <RemoveMemberButton
              memberId={member.id}
              memberLabel={name}
              organizationId={organizationId}
              onRemoved={onMutated}
            />
          </Can>
        )}
      </TableCell>
    </TableRow>
  );
}

function RemoveMemberButton({
  memberId,
  memberLabel,
  organizationId,
  onRemoved,
}: {
  memberId: string;
  memberLabel: string;
  organizationId: string;
  onRemoved?: () => Promise<void> | void;
}) {
  const [open, setOpen] = useState(false);
  const [pending, setPending] = useState(false);

  const handleConfirm = async () => {
    setPending(true);
    await authClient.organization.removeMember(
      {
        memberIdOrEmail: memberId,
        organizationId,
      },
      {
        onSuccess: async () => {
          toast.success(`${memberLabel} removido`);
          setOpen(false);
          await onRemoved?.();
        },
        onError: (e) => {
          toast.error(e.error.message ?? "Falha ao remover membro");
        },
      },
    );
    setPending(false);
  };

  return (
    <AlertDialog open={open} onOpenChange={setOpen}>
      <AlertDialogTrigger
        render={(props) => (
          <Button
            variant="ghost"
            size="icon"
            className="text-muted-foreground hover:text-destructive"
            {...props}
          >
            <Trash2 className="size-4" />
            <span className="sr-only">Remover {memberLabel}</span>
          </Button>
        )}
      />
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>Remover {memberLabel}?</AlertDialogTitle>
          <AlertDialogDescription>
            Eles perderão o acesso a esta organização. Poderão ser convidados
            novamente mais tarde.
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel disabled={pending}>Cancelar</AlertDialogCancel>
          <AlertDialogAction
            variant="destructive"
            disabled={pending}
            onClick={handleConfirm}
          >
            {pending ? "Removendo..." : "Remover"}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}

const ROLE_OPTIONS: ReadonlyArray<{ value: OrgRoles; label: string }> = [
  { value: "owner", label: "Proprietário" },
  { value: "admin", label: "Administrador" },
  { value: "operator", label: "Operador" },
  { value: "member", label: "Membro" },
];

export function ChangeRoleSelect({
  memberId,
  currentRole,
  organizationId,
  disabled,
  onChanged,
}: {
  memberId: string;
  currentRole: OrgRoles;
  organizationId: string;
  disabled?: boolean;
  onChanged?: () => Promise<void> | void;
}) {
  const handleChange = async (next: OrgRoles | null) => {
    if (!next || next === currentRole) return;
    await authClient.organization.updateMemberRole(
      {
        memberId,
        role: next,
        organizationId,
      },
      {
        onSuccess: async () => {
          toast.success(`Papel atualizado para ${next}`);
          await onChanged?.();
        },
        onError: (e) => {
          toast.error(e.error.message ?? "Falha ao atualizar o papel");
        },
      },
    );
  };

  return (
    <Select
      value={currentRole}
      onValueChange={handleChange}
      disabled={disabled}
    >
      <SelectTrigger className="h-8 w-30">
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        {ROLE_OPTIONS.map((option) => (
          <SelectItem key={option.value} value={option.value}>
            {option.label}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}
