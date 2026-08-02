import type { ConcessionOutput } from "@lindaflor/shared/schemas/concession";
import { useMutation, useQuery } from "@tanstack/react-query";
import { Loader2, Plus, Trash2 } from "lucide-react";
import React from "react";
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
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { useAppAbility } from "@/lib/ability";
import { orpc, queryClient } from "@/lib/orpc";
import { ConcessionFormDialog } from "@/routes/(auth)/cadastros/-components/concession-form-dialog";

export function ConcessionsTab() {
  const [search, setSearch] = React.useState("");
  const [dialogOpen, setDialogOpen] = React.useState(false);
  const [editingConcession, setEditingConcession] =
    React.useState<ConcessionOutput | null>(null);

  const {
    data: concessions = [],
    isPending,
    isError,
    error,
    refetch,
  } = useQuery(
    orpc.concessions.v1.getAll.queryOptions({
      select: (result) => result.data,
    }),
  );

  const normalizedSearch = search.trim().toLowerCase();
  const filteredConcessions = normalizedSearch
    ? concessions.filter((concession) =>
        concession.name.toLowerCase().includes(normalizedSearch),
      )
    : concessions;

  const openCreate = () => {
    setEditingConcession(null);
    setDialogOpen(true);
  };

  const openEdit = (concession: ConcessionOutput) => {
    setEditingConcession(concession);
    setDialogOpen(true);
  };

  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <Input
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          placeholder="Buscar por nome"
          className="max-w-64"
        />
        <Button size="sm" onClick={openCreate}>
          <Plus className="size-4" />
          Novo
        </Button>
      </div>

      {isPending ? (
        <div className="flex items-center justify-center py-10 text-muted-foreground">
          <Loader2 className="size-5 animate-spin" />
        </div>
      ) : isError ? (
        <div className="flex flex-col items-center gap-3 py-10">
          <p className="text-destructive text-sm text-center">
            {error?.message ?? "Falha ao carregar concessões"}
          </p>
          <Button variant="outline" size="sm" onClick={() => void refetch()}>
            Tentar novamente
          </Button>
        </div>
      ) : filteredConcessions.length === 0 ? (
        <p className="py-10 text-center text-sm text-muted-foreground">
          {normalizedSearch
            ? "Nenhuma concessão encontrada para essa busca"
            : "Nenhuma concessão cadastrada"}
        </p>
      ) : (
        <div className="overflow-x-auto rounded-md border">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Nome</TableHead>
                <TableHead>Estado</TableHead>
                <TableHead className="w-24 text-right">Ações</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {filteredConcessions.map((concession) => (
                <ConcessionRow
                  key={concession.id}
                  concession={concession}
                  onEdit={openEdit}
                />
              ))}
            </TableBody>
          </Table>
        </div>
      )}

      <ConcessionFormDialog
        key={editingConcession?.id ?? "new"}
        open={dialogOpen}
        onOpenChange={setDialogOpen}
        concession={editingConcession}
      />
    </div>
  );
}

function ConcessionRow({
  concession,
  onEdit,
}: {
  concession: ConcessionOutput;
  onEdit: (concession: ConcessionOutput) => void;
}) {
  const [deleteOpen, setDeleteOpen] = React.useState(false);
  const ability = useAppAbility();
  const canDelete =
    ability.can("manage", "Concessions") ||
    ability.can("delete", "Concessions");

  const deleteMutation = useMutation(
    orpc.concessions.v1.delete.mutationOptions({
      onSuccess: async () => {
        toast.success("Concessão excluída com sucesso");
        await queryClient.invalidateQueries({
          queryKey: orpc.concessions.v1.getAll.key(),
        });
        setDeleteOpen(false);
      },
      onError: (error) => {
        toast.error(error.message);
      },
    }),
  );

  return (
    <TableRow>
      <TableCell className="font-medium">{concession.name}</TableCell>
      <TableCell className="text-muted-foreground">
        {concession.state}
      </TableCell>
      <TableCell className="text-right">
        <div className="flex justify-end gap-1">
          <Button variant="ghost" size="sm" onClick={() => onEdit(concession)}>
            Editar
          </Button>
          {canDelete && (
            <AlertDialog open={deleteOpen} onOpenChange={setDeleteOpen}>
              <AlertDialogTrigger
                render={(props) => (
                  <Button
                    variant="ghost"
                    size="icon"
                    className="text-muted-foreground hover:text-destructive"
                    {...props}
                  >
                    <Trash2 className="size-4" />
                    <span className="sr-only">Excluir {concession.name}</span>
                  </Button>
                )}
              />
              <AlertDialogContent>
                <AlertDialogHeader>
                  <AlertDialogTitle>
                    Excluir concessão &quot;{concession.name}&quot;?
                  </AlertDialogTitle>
                  <AlertDialogDescription>
                    Esta ação não pode ser desfeita. Isso excluirá
                    permanentemente o registro da concessão.
                  </AlertDialogDescription>
                </AlertDialogHeader>
                <AlertDialogFooter>
                  <AlertDialogCancel disabled={deleteMutation.isPending}>
                    Cancelar
                  </AlertDialogCancel>
                  <AlertDialogAction
                    variant="destructive"
                    disabled={deleteMutation.isPending}
                    onClick={() =>
                      deleteMutation.mutate({ ids: [concession.id] })
                    }
                  >
                    {deleteMutation.isPending ? "Excluindo…" : "Excluir"}
                  </AlertDialogAction>
                </AlertDialogFooter>
              </AlertDialogContent>
            </AlertDialog>
          )}
        </div>
      </TableCell>
    </TableRow>
  );
}
