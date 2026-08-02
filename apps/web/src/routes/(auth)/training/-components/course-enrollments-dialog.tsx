import type { TrainingCourseOutput } from "@lindaflor/shared/schemas/training";
import { useMutation, useQuery } from "@tanstack/react-query";
import { format } from "date-fns";
import { Loader2, Trash2, UserPlus } from "lucide-react";
import { useMemo, useState } from "react";
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
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { authClient } from "@/lib/auth-client";
import { orpc } from "@/lib/orpc";

type CourseEnrollmentsDialogProps = {
  course: TrainingCourseOutput | null;
  open: boolean;
  onOpenChange: (open: boolean) => void;
};

const initialsOf = (name: string | null | undefined) => {
  if (!name) return "?";
  const parts = name.trim().split(/\s+/u);
  const first = parts[0]?.[0] ?? "";
  const second = parts.length > 1 ? (parts.at(-1)?.[0] ?? "") : "";
  return (first + second).toUpperCase() || "?";
};

const RoleBadge = ({ role }: { role: string }) => {
  if (role === "owner") return <Badge>Dono</Badge>;
  if (role === "admin") return <Badge variant="secondary">Administrador</Badge>;
  if (role === "operator") return <Badge variant="secondary">Operador</Badge>;
  return <Badge variant="outline">Membro</Badge>;
};

function RemoveEnrollmentButton({
  courseId,
  userId,
  userName,
  onRemoved,
}: {
  courseId: string;
  userId: string;
  userName: string;
  onRemoved: () => void;
}) {
  const deleteMutation = useMutation(
    orpc.training.v1.enrollments.delete.mutationOptions({
      onSuccess: () => {
        toast.success("Matrícula removida");
        onRemoved();
      },
      onError: (error) => toast.error(error.message),
    }),
  );

  return (
    <AlertDialog>
      <AlertDialogTrigger
        render={
          <Button
            variant="ghost"
            size="sm"
            aria-label={`Remover matrícula de ${userName}`}
          >
            <Trash2 className="size-4 text-destructive" />
          </Button>
        }
      />
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>Remover matrícula</AlertDialogTitle>
          <AlertDialogDescription>
            Tem certeza que deseja remover a matrícula de &quot;{userName}
            &quot;? O usuário perderá acesso ao curso na aba Meus cursos.
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel>Cancelar</AlertDialogCancel>
          <AlertDialogAction
            variant="destructive"
            disabled={deleteMutation.isPending}
            onClick={() =>
              deleteMutation.mutate({ course_id: courseId, user_id: userId })
            }
          >
            Remover
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}

export function CourseEnrollmentsDialog({
  course,
  open,
  onOpenChange,
}: CourseEnrollmentsDialogProps) {
  const [selectedUserId, setSelectedUserId] = useState("");
  const { data: activeOrganization } = authClient.useActiveOrganization();

  const {
    data: enrollments = [],
    refetch,
    isLoading,
  } = useQuery({
    ...orpc.training.v1.enrollments.list.queryOptions({
      input: { course_id: course?.id ?? "" },
      select: (result) => result.data,
    }),
    enabled: open && course !== null,
  });

  const createEnrollment = useMutation(
    orpc.training.v1.enrollments.create.mutationOptions({
      onSuccess: async () => {
        toast.success("Usuário matriculado");
        setSelectedUserId("");
        await refetch();
      },
      onError: (error) => toast.error(error.message),
    }),
  );

  const enrolledUserIds = useMemo(
    () => new Set(enrollments.map((row) => row.user_id)),
    [enrollments],
  );

  const availableMembers = useMemo(
    () =>
      (activeOrganization?.members ?? []).filter(
        (member) => !enrolledUserIds.has(member.userId),
      ),
    [activeOrganization?.members, enrolledUserIds],
  );

  const handleAssign = () => {
    if (!course || !selectedUserId) return;
    createEnrollment.mutate({
      course_id: course.id,
      user_id: selectedUserId,
    });
  };

  const handleOpenChange = (nextOpen: boolean) => {
    if (!nextOpen) {
      setSelectedUserId("");
    }
    onOpenChange(nextOpen);
  };

  if (!course) {
    return null;
  }

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent className="flex max-h-[85vh] flex-col overflow-hidden sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>Matrículas do curso</DialogTitle>
          <DialogDescription>
            Atribua &quot;{course.title}&quot; a membros da organização.
          </DialogDescription>
        </DialogHeader>

        <div className="flex flex-col gap-2 sm:flex-row sm:items-end">
          <div className="min-w-0 flex-1 space-y-1">
            <p className="text-sm font-medium">Adicionar usuário</p>
            <Select
              value={selectedUserId}
              onValueChange={(value) => {
                if (typeof value === "string") {
                  setSelectedUserId(value);
                }
              }}
              disabled={
                availableMembers.length === 0 || createEnrollment.isPending
              }
            >
              <SelectTrigger className="w-full">
                <SelectValue placeholder="Selecione um membro">
                  {(value) => {
                    const member = availableMembers.find(
                      (m) => m.userId === value,
                    );
                    return member?.user?.name ?? member?.user?.email ?? value;
                  }}
                </SelectValue>
              </SelectTrigger>
              <SelectContent>
                {availableMembers.map((member) => (
                  <SelectItem key={member.userId} value={member.userId}>
                    {member.user?.name ?? member.user?.email ?? member.userId}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
          <Button
            className="shrink-0"
            disabled={!selectedUserId || createEnrollment.isPending}
            onClick={handleAssign}
          >
            {createEnrollment.isPending ? (
              <Loader2 className="size-4 animate-spin" />
            ) : (
              <UserPlus className="size-4" />
            )}
            Atribuir
          </Button>
        </div>

        {availableMembers.length === 0 && !isLoading ? (
          <p className="text-sm text-muted-foreground">
            {enrollments.length > 0
              ? "Todos os membros da organização já estão matriculados."
              : "Nenhum membro disponível para matrícula."}
          </p>
        ) : null}

        <div className="min-h-0 flex-1 overflow-auto rounded-md border">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Membro</TableHead>
                <TableHead>Email</TableHead>
                <TableHead>Papel</TableHead>
                <TableHead>Matriculado em</TableHead>
                <TableHead className="text-right">Ações</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {isLoading ? (
                ["row-a", "row-b", "row-c"].map((key) => (
                  <TableRow key={key}>
                    <TableCell colSpan={5}>
                      <Skeleton className="h-8 w-full" />
                    </TableCell>
                  </TableRow>
                ))
              ) : enrollments.length === 0 ? (
                <TableRow>
                  <TableCell
                    colSpan={5}
                    className="py-8 text-center text-muted-foreground"
                  >
                    Nenhum usuário matriculado
                  </TableCell>
                </TableRow>
              ) : (
                enrollments.map((enrollment) => (
                  <TableRow key={enrollment.id}>
                    <TableCell>
                      <div className="flex items-center gap-2">
                        <Avatar className="size-8">
                          <AvatarFallback>
                            {initialsOf(enrollment.user_name)}
                          </AvatarFallback>
                        </Avatar>
                        <span className="font-medium">
                          {enrollment.user_name}
                        </span>
                      </div>
                    </TableCell>
                    <TableCell className="text-muted-foreground">
                      {enrollment.user_email}
                    </TableCell>
                    <TableCell>
                      <RoleBadge role={enrollment.member_role} />
                    </TableCell>
                    <TableCell className="text-muted-foreground">
                      {format(enrollment.enrolled_at, "dd/MM/yyyy")}
                    </TableCell>
                    <TableCell className="text-right">
                      <RemoveEnrollmentButton
                        courseId={course.id}
                        userId={enrollment.user_id}
                        userName={enrollment.user_name}
                        onRemoved={() => void refetch()}
                      />
                    </TableCell>
                  </TableRow>
                ))
              )}
            </TableBody>
          </Table>
        </div>
      </DialogContent>
    </Dialog>
  );
}
