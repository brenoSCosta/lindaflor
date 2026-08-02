import type { InstallationOutput } from "@lindaflor/shared/schemas/installation";
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
import { InstallationFormDialog } from "@/routes/(auth)/cadastros/-components/installation-form-dialog";

export function InstallationsTab() {
  const [search, setSearch] = React.useState("");
  const [dialogOpen, setDialogOpen] = React.useState(false);
  const [editingInstallation, setEditingInstallation] =
    React.useState<InstallationOutput | null>(null);

  const {
    data: installations = [],
    isPending,
    isError,
    error,
    refetch,
  } = useQuery(
    orpc.installations.v1.getAll.queryOptions({
      select: (result) => result.data,
    }),
  );

  const { data: concessions = [] } = useQuery(
    orpc.concessions.v1.getAll.queryOptions({
      select: (result) => result.data,
    }),
  );

  const concessionNameById = React.useMemo(
    () =>
      new Map(
        concessions.map((concession) => [concession.id, concession.name]),
      ),
    [concessions],
  );

  const normalizedSearch = search.trim().toLowerCase();
  const filteredInstallations = normalizedSearch
    ? installations.filter((installation) =>
        installation.name.toLowerCase().includes(normalizedSearch),
      )
    : installations;

  const openCreate = () => {
    setEditingInstallation(null);
    setDialogOpen(true);
  };

  const openEdit = (installation: InstallationOutput) => {
    setEditingInstallation(installation);
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
          Nova instalação
        </Button>
      </div>

      {isPending ? (
        <div className="flex items-center justify-center py-10 text-muted-foreground">
          <Loader2 className="size-5 animate-spin" />
        </div>
      ) : isError ? (
        <div className="flex flex-col items-center gap-3 py-10">
          <p className="text-destructive text-sm text-center">
            {error?.message ?? "Falha ao carregar instalações"}
          </p>
          <Button variant="outline" size="sm" onClick={() => void refetch()}>
            Tentar novamente
          </Button>
        </div>
      ) : filteredInstallations.length === 0 ? (
        <p className="py-10 text-center text-sm text-muted-foreground">
          {normalizedSearch
            ? "Nenhuma instalação encontrada para essa busca"
            : "Nenhuma instalação cadastrada"}
        </p>
      ) : (
        <div className="overflow-x-auto rounded-md border">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Nome</TableHead>
                <TableHead>Concessão</TableHead>
                <TableHead className="w-24 text-right">Ações</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {filteredInstallations.map((installation) => (
                <InstallationRow
                  key={installation.id}
                  installation={installation}
                  concessionName={concessionNameById.get(
                    installation.concession_id,
                  )}
                  onEdit={openEdit}
                />
              ))}
            </TableBody>
          </Table>
        </div>
      )}

      <InstallationFormDialog
        key={editingInstallation?.id ?? "new"}
        open={dialogOpen}
        onOpenChange={setDialogOpen}
        installation={editingInstallation}
      />
    </div>
  );
}

function InstallationRow({
  installation,
  concessionName,
  onEdit,
}: {
  installation: InstallationOutput;
  concessionName: string | undefined;
  onEdit: (installation: InstallationOutput) => void;
}) {
  const [deleteOpen, setDeleteOpen] = React.useState(false);
  const ability = useAppAbility();
  const canDelete =
    ability.can("manage", "Installations") ||
    ability.can("delete", "Installations");

  const deleteMutation = useMutation(
    orpc.installations.v1.delete.mutationOptions({
      onSuccess: async () => {
        toast.success("Instalação excluída com sucesso");
        await queryClient.invalidateQueries({
          queryKey: orpc.installations.v1.getAll.key(),
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
      <TableCell className="font-medium">{installation.name}</TableCell>
      <TableCell className="text-muted-foreground">
        {concessionName ?? "—"}
      </TableCell>
      <TableCell className="text-right">
        <div className="flex justify-end gap-1">
          <Button
            variant="ghost"
            size="sm"
            onClick={() => onEdit(installation)}
          >
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
                    <span className="sr-only">Excluir {installation.name}</span>
                  </Button>
                )}
              />
              <AlertDialogContent>
                <AlertDialogHeader>
                  <AlertDialogTitle>
                    Excluir instalação &quot;{installation.name}&quot;?
                  </AlertDialogTitle>
                  <AlertDialogDescription>
                    Esta ação não pode ser desfeita. Isso excluirá
                    permanentemente o registro da instalação.
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
                      deleteMutation.mutate({ ids: [installation.id] })
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
