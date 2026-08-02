import {
  MEASUREMENT_EQUIPMENT_TYPE_LABELS,
  type MeasurementEquipmentOutput,
} from "@lindaflor/shared/schemas/measurement-equipment";
import { useMutation, useQuery } from "@tanstack/react-query";
import { Loader2, Plus, Trash2, X } from "lucide-react";
import React from "react";
import { toast } from "sonner";
import { useDebounceValue } from "usehooks-ts";

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
import {
  InputGroup,
  InputGroupButton,
  InputGroupInput,
} from "@/components/ui/input-group";
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
import { MeasurementEquipmentFormDialog } from "@/routes/(auth)/cadastros/-components/measurement-equipment-form-dialog";

export function MeasurementEquipmentsTab() {
  const [search, setSearch] = React.useState("");
  const [debouncedSearch] = useDebounceValue(search, 300);
  const [dialogOpen, setDialogOpen] = React.useState(false);
  const [editingEquipment, setEditingEquipment] =
    React.useState<MeasurementEquipmentOutput | null>(null);

  const {
    data: equipments = [],
    isPending,
    isError,
    error,
    refetch,
  } = useQuery(
    orpc.measurementEquipments.v1.getAll.queryOptions({
      input: {
        globalFilter: debouncedSearch || undefined,
        pagination: { pageIndex: 0, pageSize: 500 },
      },
      select: (result) => result.data,
    }),
  );

  const openCreate = () => {
    setEditingEquipment(null);
    setDialogOpen(true);
  };

  const openEdit = (equipment: MeasurementEquipmentOutput) => {
    setEditingEquipment(equipment);
    setDialogOpen(true);
  };

  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <InputGroup className="max-w-64">
          <InputGroupInput
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder="Buscar por código"
          />
          {search && (
            <InputGroupButton
              size="icon-xs"
              onClick={() => setSearch("")}
              aria-label="Limpar busca"
            >
              <X className="size-4" />
            </InputGroupButton>
          )}
        </InputGroup>
        <Button size="sm" onClick={openCreate}>
          <Plus className="size-4" />
          Nova trena
        </Button>
      </div>

      {isPending ? (
        <div className="flex items-center justify-center py-10 text-muted-foreground">
          <Loader2 className="size-5 animate-spin" />
        </div>
      ) : isError ? (
        <div className="flex flex-col items-center gap-3 py-10">
          <p className="text-destructive text-sm text-center">
            {error?.message ?? "Falha ao carregar trenas"}
          </p>
          <Button variant="outline" size="sm" onClick={() => void refetch()}>
            Tentar novamente
          </Button>
        </div>
      ) : equipments.length === 0 ? (
        <p className="py-10 text-center text-sm text-muted-foreground">
          {debouncedSearch
            ? "Nenhuma trena encontrada para essa busca"
            : "Nenhuma trena cadastrada"}
        </p>
      ) : (
        <div className="overflow-x-auto rounded-md border">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Código</TableHead>
                <TableHead>Tipo</TableHead>
                <TableHead>Comprimento</TableHead>
                <TableHead>Descrição</TableHead>
                <TableHead>Status</TableHead>
                <TableHead className="w-24 text-right">Ações</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {equipments.map((equipment) => (
                <EquipmentRow
                  key={equipment.id}
                  equipment={equipment}
                  onEdit={openEdit}
                />
              ))}
            </TableBody>
          </Table>
        </div>
      )}

      <MeasurementEquipmentFormDialog
        key={editingEquipment?.id ?? "new"}
        open={dialogOpen}
        onOpenChange={setDialogOpen}
        equipment={editingEquipment}
      />
    </div>
  );
}

function EquipmentRow({
  equipment,
  onEdit,
}: {
  equipment: MeasurementEquipmentOutput;
  onEdit: (equipment: MeasurementEquipmentOutput) => void;
}) {
  const [deleteOpen, setDeleteOpen] = React.useState(false);
  const ability = useAppAbility();
  const canDelete =
    ability.can("manage", "MeasurementEquipments") ||
    ability.can("delete", "MeasurementEquipments");

  const deleteMutation = useMutation(
    orpc.measurementEquipments.v1.delete.mutationOptions({
      onSuccess: async () => {
        toast.success("Trena excluída com sucesso");
        await queryClient.invalidateQueries({
          queryKey: orpc.measurementEquipments.v1.getAll.key(),
        });
        setDeleteOpen(false);
      },
      onError: (error) => {
        toast.error(error.message);
      },
    }),
  );

  return (
    <TableRow className={equipment.active ? undefined : "opacity-60"}>
      <TableCell className="font-medium">{equipment.code}</TableCell>
      <TableCell>{MEASUREMENT_EQUIPMENT_TYPE_LABELS[equipment.type]}</TableCell>
      <TableCell>
        {equipment.length_m != null ? `${equipment.length_m} m` : "—"}
      </TableCell>
      <TableCell className="text-muted-foreground">
        {equipment.description ?? "—"}
      </TableCell>
      <TableCell>{equipment.active ? "Ativa" : "Inativa"}</TableCell>
      <TableCell className="text-right">
        <div className="flex justify-end gap-1">
          <Button variant="ghost" size="sm" onClick={() => onEdit(equipment)}>
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
                    <span className="sr-only">Excluir {equipment.code}</span>
                  </Button>
                )}
              />
              <AlertDialogContent>
                <AlertDialogHeader>
                  <AlertDialogTitle>
                    Excluir trena &quot;{equipment.code}&quot;?
                  </AlertDialogTitle>
                  <AlertDialogDescription>
                    Tanques e medições que usam esta trena ficarão sem
                    equipamento vinculado.
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
                      deleteMutation.mutate({ ids: [equipment.id] })
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
