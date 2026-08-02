import type { TankOutput } from "@lindaflor/shared/schemas/tankage/tanks";
import { useMutation, useQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
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
import { Can } from "@/lib/ability";
import { orpc, queryClient } from "@/lib/orpc";
import { todayIsoDate } from "@/lib/utils";
import { TankCalibrationStatusBadge } from "@/routes/(auth)/arqueacao/-components/tank-calibration-status-badge";
import { TankFormDialog } from "@/routes/(auth)/cadastros/-components/tank-form-dialog";

export function TanksTab() {
  const [search, setSearch] = React.useState("");
  const [dialogOpen, setDialogOpen] = React.useState(false);
  const [editingTank, setEditingTank] = React.useState<TankOutput | null>(null);

  const {
    data: tanks = [],
    isPending,
    isError,
    error,
    refetch,
  } = useQuery(
    orpc.tanks.v1.tank.list.all.queryOptions({
      select: (result) => result.data,
    }),
  );

  const { data: currentCalibrations = [] } = useQuery(
    orpc.tanks.v1.calibration.list.current.queryOptions({
      input: { at: todayIsoDate() },
      select: (result) => result.data,
    }),
  );

  const currentByTankId = React.useMemo(() => {
    const map = new Map<string, string | null>();
    for (const row of currentCalibrations) {
      map.set(row.tank_id, row.valid_until);
    }
    return map;
  }, [currentCalibrations]);

  const normalizedSearch = search.trim().toLowerCase();
  const filteredTanks = normalizedSearch
    ? tanks.filter((tank) => tank.tag.toLowerCase().includes(normalizedSearch))
    : tanks;

  const openCreate = () => {
    setEditingTank(null);
    setDialogOpen(true);
  };

  const openEdit = (tank: TankOutput) => {
    setEditingTank(tank);
    setDialogOpen(true);
  };

  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <Input
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          placeholder="Buscar por TAG"
          className="max-w-64"
        />
        <Button size="sm" onClick={openCreate}>
          <Plus className="size-4" />
          Novo tanque
        </Button>
      </div>

      {isPending ? (
        <div className="flex items-center justify-center py-10 text-muted-foreground">
          <Loader2 className="size-5 animate-spin" />
        </div>
      ) : isError ? (
        <div className="flex flex-col items-center gap-3 py-10">
          <p className="text-destructive text-sm text-center">
            {error?.message ?? "Falha ao carregar tanques"}
          </p>
          <Button variant="outline" size="sm" onClick={() => void refetch()}>
            Tentar novamente
          </Button>
        </div>
      ) : filteredTanks.length === 0 ? (
        <p className="py-10 text-center text-sm text-muted-foreground">
          {normalizedSearch
            ? "Nenhum tanque encontrado para essa busca"
            : "Nenhum tanque cadastrado"}
        </p>
      ) : (
        <div className="overflow-x-auto rounded-md border">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>TAG</TableHead>
                <TableHead>Concessão</TableHead>
                <TableHead>Instalação</TableHead>
                <TableHead>Arqueação</TableHead>
                <TableHead className="w-40 text-right">Ações</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {filteredTanks.map((tank) => (
                <TankRow
                  key={tank.id}
                  tank={tank}
                  calibrationValidUntil={
                    currentByTankId.has(tank.id)
                      ? currentByTankId.get(tank.id)
                      : undefined
                  }
                  onEdit={openEdit}
                />
              ))}
            </TableBody>
          </Table>
        </div>
      )}

      <TankFormDialog
        key={editingTank?.id ?? "new"}
        open={dialogOpen}
        onOpenChange={setDialogOpen}
        tank={editingTank}
      />
    </div>
  );
}

function TankRow({
  tank,
  calibrationValidUntil,
  onEdit,
}: {
  tank: TankOutput;
  calibrationValidUntil: string | null | undefined;
  onEdit: (tank: TankOutput) => void;
}) {
  const [deleteOpen, setDeleteOpen] = React.useState(false);

  const deleteMutation = useMutation(
    orpc.tanks.v1.tank.delete.mutationOptions({
      onSuccess: async () => {
        toast.success("Tanque excluído com sucesso");
        await queryClient.invalidateQueries({
          queryKey: orpc.tanks.v1.tank.list.all.key(),
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
      <TableCell className="font-medium">{tank.tag}</TableCell>
      <TableCell className="text-muted-foreground">
        {tank.concession_name}
      </TableCell>
      <TableCell className="text-muted-foreground">
        {tank.installation_name}
      </TableCell>
      <TableCell>
        <Can I="read" a="TankCalibrations">
          <Link
            to="/arqueacao"
            search={{ tank_id: tank.id }}
            className="inline-flex"
          >
            <TankCalibrationStatusBadge validUntil={calibrationValidUntil} />
          </Link>
        </Can>
      </TableCell>
      <TableCell className="text-right">
        <div className="flex justify-end gap-1">
          <Can I="read" a="TankCalibrations">
            <Button
              variant="ghost"
              size="sm"
              render={<Link to="/arqueacao" search={{ tank_id: tank.id }} />}
            >
              Arqueação
            </Button>
          </Can>
          <Button variant="ghost" size="sm" onClick={() => onEdit(tank)}>
            Editar
          </Button>
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
                  <span className="sr-only">Excluir {tank.tag}</span>
                </Button>
              )}
            />
            <AlertDialogContent>
              <AlertDialogHeader>
                <AlertDialogTitle>
                  Excluir tanque &quot;{tank.tag}&quot;?
                </AlertDialogTitle>
                <AlertDialogDescription>
                  Esta ação não pode ser desfeita. Isso excluirá permanentemente
                  o registro do tanque.
                </AlertDialogDescription>
              </AlertDialogHeader>
              <AlertDialogFooter>
                <AlertDialogCancel disabled={deleteMutation.isPending}>
                  Cancelar
                </AlertDialogCancel>
                <AlertDialogAction
                  variant="destructive"
                  disabled={deleteMutation.isPending}
                  onClick={() => deleteMutation.mutate({ ids: [tank.id] })}
                >
                  {deleteMutation.isPending ? "Excluindo…" : "Excluir"}
                </AlertDialogAction>
              </AlertDialogFooter>
            </AlertDialogContent>
          </AlertDialog>
        </div>
      </TableCell>
    </TableRow>
  );
}
