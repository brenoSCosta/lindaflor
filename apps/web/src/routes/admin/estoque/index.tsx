import { productSizeLabels } from "@lindaflor/shared/enums/commerce";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { useMemo, useRef, useState } from "react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { Textarea } from "@/components/ui/textarea";
import { orpc } from "@/lib/orpc";

export const Route = createFileRoute("/admin/estoque/")({
  component: AdminInventoryPage,
});

type Tab = "saldo" | "entrada" | "movimentos" | "depositos" | "importar";

function AdminInventoryPage() {
  const [tab, setTab] = useState<Tab>("saldo");
  const [warehouseFilter, setWarehouseFilter] = useState<string>("all");
  const [lowStockOnly, setLowStockOnly] = useState(false);
  const [adjustingId, setAdjustingId] = useState<string | null>(null);
  const [delta, setDelta] = useState("0");
  const [receiveForm, setReceiveForm] = useState({
    variant_id: "",
    warehouse_id: "",
    quantity: "1",
    notes: "",
  });
  const [transferForm, setTransferForm] = useState({
    variant_id: "",
    from_warehouse_id: "",
    to_warehouse_id: "",
    quantity: "1",
    notes: "",
  });
  const [warehouseForm, setWarehouseForm] = useState({ code: "", name: "" });
  const [csvText, setCsvText] = useState("");
  const fileInputRef = useRef<HTMLInputElement>(null);
  const queryClient = useQueryClient();

  const warehousesQuery = useQuery(
    orpc.commerce.admin.listWarehouses.queryOptions({ input: undefined }),
  );

  const inventoryQuery = useQuery(
    orpc.commerce.admin.listInventory.queryOptions({
      input: {
        warehouse_id: warehouseFilter === "all" ? undefined : warehouseFilter,
        low_stock_only: lowStockOnly || undefined,
      },
    }),
  );

  const alertsQuery = useQuery(
    orpc.commerce.admin.listLowStockAlerts.queryOptions({ input: undefined }),
  );

  const movementsQuery = useQuery(
    orpc.commerce.admin.listInventoryMovements.queryOptions({
      input: { limit: 100, offset: 0 },
    }),
  );

  const invalidate = async () => {
    await queryClient.invalidateQueries();
  };

  const adjustMutation = useMutation(
    orpc.commerce.admin.adjustInventory.mutationOptions({
      onSuccess: async () => {
        await invalidate();
        toast.success("Estoque ajustado");
        setAdjustingId(null);
        setDelta("0");
      },
      onError: (error) => toast.error(error.message),
    }),
  );

  const receiveMutation = useMutation(
    orpc.commerce.admin.receiveInventory.mutationOptions({
      onSuccess: async () => {
        await invalidate();
        toast.success("Entrada registrada");
        setReceiveForm({
          variant_id: "",
          warehouse_id: "",
          quantity: "1",
          notes: "",
        });
      },
      onError: (error) => toast.error(error.message),
    }),
  );

  const transferMutation = useMutation(
    orpc.commerce.admin.transferInventory.mutationOptions({
      onSuccess: async () => {
        await invalidate();
        toast.success("Transferência concluída");
      },
      onError: (error) => toast.error(error.message),
    }),
  );

  const warehouseMutation = useMutation(
    orpc.commerce.admin.createWarehouse.mutationOptions({
      onSuccess: async () => {
        await invalidate();
        toast.success("Depósito criado");
        setWarehouseForm({ code: "", name: "" });
      },
      onError: (error) => toast.error(error.message),
    }),
  );

  const importMutation = useMutation(
    orpc.commerce.admin.importInventoryCsv.mutationOptions({
      onSuccess: async (result) => {
        await invalidate();
        toast.success(
          `Importação: ${result.updated} atualizados, ${result.errors} erros`,
        );
        setCsvText("");
      },
      onError: (error) => toast.error(error.message),
    }),
  );

  const exportMutation = useMutation(
    orpc.commerce.admin.exportInventoryCsv.mutationOptions({
      onSuccess: (result) => {
        const blob = new Blob([result.content], {
          type: "text/csv;charset=utf-8",
        });
        const url = URL.createObjectURL(blob);
        const link = document.createElement("a");
        link.href = url;
        link.download = result.filename;
        link.click();
        URL.revokeObjectURL(url);
        toast.success("CSV exportado");
      },
      onError: (error) => toast.error(error.message),
    }),
  );

  const variantOptions = useMemo(
    () => inventoryQuery.data?.data ?? [],
    [inventoryQuery.data?.data],
  );

  const tabs: { id: Tab; label: string }[] = [
    { id: "saldo", label: "Saldo" },
    { id: "entrada", label: "Entrada" },
    { id: "movimentos", label: "Movimentações" },
    { id: "depositos", label: "Depósitos" },
    { id: "importar", label: "Importar / Exportar" },
  ];

  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div>
          <h2 className="text-2xl font-semibold">Estoque</h2>
          <p className="text-stone-600">
            Saldo por depósito, entradas, transferências e sincronização CSV.
          </p>
        </div>
        <Button
          variant="outline"
          onClick={() => exportMutation.mutate(undefined)}
          disabled={exportMutation.isPending}
        >
          Exportar CSV
        </Button>
      </div>

      {(alertsQuery.data?.data.length ?? 0) > 0 ? (
        <div className="rounded-xl border border-amber-200 bg-amber-50 p-4">
          <p className="font-medium text-amber-900">
            {alertsQuery.data?.data.length} alerta(s) de estoque baixo
          </p>
          <p className="mt-1 text-sm text-amber-800">
            Limite global: {alertsQuery.data?.threshold} un. · revise os itens
            em destaque abaixo.
          </p>
          <ul className="mt-3 space-y-1 text-sm text-amber-900">
            {alertsQuery.data?.data.slice(0, 5).map((item) => (
              <li key={`${item.variant_id}-${item.warehouse_id}`}>
                {item.product_name} ({item.sku}) — {item.available} disponível
                em {item.warehouse_name}
              </li>
            ))}
          </ul>
        </div>
      ) : null}

      <div className="flex flex-wrap gap-2 border-b pb-2">
        {tabs.map((item) => (
          <Button
            key={item.id}
            variant={tab === item.id ? "default" : "ghost"}
            size="sm"
            onClick={() => setTab(item.id)}
          >
            {item.label}
          </Button>
        ))}
      </div>

      {tab === "saldo" ? (
        <div className="space-y-4">
          <div className="flex flex-wrap gap-3">
            <select
              aria-label="Filtrar por depósito"
              className="h-9 rounded-md border bg-white px-3 text-sm"
              value={warehouseFilter}
              onChange={(e) => setWarehouseFilter(e.target.value)}
            >
              <option value="all">Todos os depósitos</option>
              {warehousesQuery.data?.data.map((w) => (
                <option key={w.id} value={w.id}>
                  {w.name} ({w.code})
                </option>
              ))}
            </select>
            <label className="flex items-center gap-2 text-sm text-stone-600">
              <input
                type="checkbox"
                checked={lowStockOnly}
                onChange={(e) => setLowStockOnly(e.target.checked)}
              />
              Só estoque baixo
            </label>
          </div>

          <InventoryTable
            loading={inventoryQuery.isLoading}
            error={inventoryQuery.isError}
            items={inventoryQuery.data?.data ?? []}
            adjustingId={adjustingId}
            delta={delta}
            onDeltaChange={setDelta}
            onStartAdjust={(id) => {
              setAdjustingId(id);
              setDelta("0");
            }}
            onConfirmAdjust={(variantId, warehouseId) =>
              adjustMutation.mutate({
                variant_id: variantId,
                warehouse_id: warehouseId,
                quantity_delta: Number(delta),
              })
            }
          />
        </div>
      ) : null}

      {tab === "entrada" ? (
        <form
          className="max-w-lg space-y-4 rounded-xl border bg-white p-6"
          onSubmit={(e) => {
            e.preventDefault();
            if (!receiveForm.variant_id) {
              toast.error("Selecione uma variante");
              return;
            }
            receiveMutation.mutate({
              variant_id: receiveForm.variant_id,
              warehouse_id: receiveForm.warehouse_id || undefined,
              quantity: Number(receiveForm.quantity),
              notes: receiveForm.notes || undefined,
            });
          }}
        >
          <h3 className="font-medium">Entrada de mercadoria</h3>
          <div className="space-y-2">
            <Label htmlFor="receive-variant">Variante (SKU)</Label>
            <select
              id="receive-variant"
              aria-label="Variante (SKU)"
              className="h-10 w-full rounded-md border px-3 text-sm"
              value={receiveForm.variant_id}
              onChange={(e) =>
                setReceiveForm((f) => ({ ...f, variant_id: e.target.value }))
              }
              required
            >
              <option value="">Selecione…</option>
              {variantOptions.map((item) => (
                <option
                  key={`${item.variant_id}-${item.warehouse_id}`}
                  value={item.variant_id}
                >
                  {item.sku} · {item.product_name} (
                  {productSizeLabels[item.size]})
                </option>
              ))}
            </select>
          </div>
          <div className="space-y-2">
            <Label htmlFor="receive-warehouse">Depósito</Label>
            <select
              id="receive-warehouse"
              aria-label="Depósito"
              className="h-10 w-full rounded-md border px-3 text-sm"
              value={receiveForm.warehouse_id}
              onChange={(e) =>
                setReceiveForm((f) => ({ ...f, warehouse_id: e.target.value }))
              }
            >
              <option value="">Padrão (loja principal)</option>
              {warehousesQuery.data?.data.map((w) => (
                <option key={w.id} value={w.id}>
                  {w.name}
                </option>
              ))}
            </select>
          </div>
          <div className="space-y-2">
            <Label>Quantidade</Label>
            <Input
              type="number"
              min={1}
              value={receiveForm.quantity}
              onChange={(e) =>
                setReceiveForm((f) => ({ ...f, quantity: e.target.value }))
              }
            />
          </div>
          <div className="space-y-2">
            <Label>Observações</Label>
            <Textarea
              value={receiveForm.notes}
              onChange={(e) =>
                setReceiveForm((f) => ({ ...f, notes: e.target.value }))
              }
              placeholder="NF, fornecedor, coleção..."
            />
          </div>
          <Button type="submit" disabled={receiveMutation.isPending}>
            Registrar entrada
          </Button>
        </form>
      ) : null}

      {tab === "movimentos" ? (
        <div className="overflow-hidden rounded-xl border bg-white">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Data</TableHead>
                <TableHead>Tipo</TableHead>
                <TableHead>Produto</TableHead>
                <TableHead>SKU</TableHead>
                <TableHead>Depósito</TableHead>
                <TableHead>Qtd</TableHead>
                <TableHead>Notas</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {movementsQuery.data?.data.map((m) => (
                <TableRow key={m.id}>
                  <TableCell className="text-xs text-stone-500">
                    {new Date(m.created_at).toLocaleString("pt-BR")}
                  </TableCell>
                  <TableCell className="capitalize">{m.type}</TableCell>
                  <TableCell>{m.product_name}</TableCell>
                  <TableCell>{m.sku}</TableCell>
                  <TableCell>{m.warehouse_name ?? "—"}</TableCell>
                  <TableCell
                    className={
                      m.quantity < 0 ? "text-red-600" : "text-emerald-700"
                    }
                  >
                    {m.quantity > 0 ? `+${m.quantity}` : m.quantity}
                  </TableCell>
                  <TableCell className="max-w-xs truncate text-xs text-stone-500">
                    {m.notes ?? "—"}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </div>
      ) : null}

      {tab === "depositos" ? (
        <div className="grid gap-6 lg:grid-cols-2">
          <div className="overflow-hidden rounded-xl border bg-white">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Código</TableHead>
                  <TableHead>Nome</TableHead>
                  <TableHead>Padrão</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {warehousesQuery.data?.data.map((w) => (
                  <TableRow key={w.id}>
                    <TableCell className="font-mono text-sm">
                      {w.code}
                    </TableCell>
                    <TableCell>{w.name}</TableCell>
                    <TableCell>{w.is_default ? "Sim" : "—"}</TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </div>

          <form
            className="space-y-4 rounded-xl border bg-white p-6"
            onSubmit={(e) => {
              e.preventDefault();
              warehouseMutation.mutate(warehouseForm);
            }}
          >
            <h3 className="font-medium">Novo depósito</h3>
            <div className="space-y-2">
              <Label>Código</Label>
              <Input
                value={warehouseForm.code}
                onChange={(e) =>
                  setWarehouseForm((f) => ({ ...f, code: e.target.value }))
                }
                placeholder="estoque-sp"
                required
              />
            </div>
            <div className="space-y-2">
              <Label>Nome</Label>
              <Input
                value={warehouseForm.name}
                onChange={(e) =>
                  setWarehouseForm((f) => ({ ...f, name: e.target.value }))
                }
                placeholder="Estoque São Paulo"
                required
              />
            </div>
            <Button type="submit" disabled={warehouseMutation.isPending}>
              Criar depósito
            </Button>
          </form>

          <form
            className="space-y-4 rounded-xl border bg-white p-6 lg:col-span-2"
            onSubmit={(e) => {
              e.preventDefault();
              if (
                !transferForm.variant_id ||
                !transferForm.from_warehouse_id ||
                !transferForm.to_warehouse_id
              ) {
                toast.error("Preencha todos os campos da transferência");
                return;
              }
              transferMutation.mutate({
                variant_id: transferForm.variant_id,
                from_warehouse_id: transferForm.from_warehouse_id,
                to_warehouse_id: transferForm.to_warehouse_id,
                quantity: Number(transferForm.quantity),
                notes: transferForm.notes || undefined,
              });
            }}
          >
            <h3 className="font-medium">Transferir entre depósitos</h3>
            <div className="grid gap-4 sm:grid-cols-2">
              <div className="space-y-2 sm:col-span-2">
                <Label htmlFor="transfer-variant">Variante</Label>
                <select
                  id="transfer-variant"
                  aria-label="Variante"
                  className="h-10 w-full rounded-md border px-3 text-sm"
                  value={transferForm.variant_id}
                  onChange={(e) =>
                    setTransferForm((f) => ({
                      ...f,
                      variant_id: e.target.value,
                    }))
                  }
                >
                  <option value="">Selecione…</option>
                  {variantOptions.map((item) => (
                    <option key={item.variant_id} value={item.variant_id}>
                      {item.sku} · {item.product_name}
                    </option>
                  ))}
                </select>
              </div>
              <div className="space-y-2">
                <Label htmlFor="transfer-from">Origem</Label>
                <select
                  id="transfer-from"
                  aria-label="Depósito de origem"
                  className="h-10 w-full rounded-md border px-3 text-sm"
                  value={transferForm.from_warehouse_id}
                  onChange={(e) =>
                    setTransferForm((f) => ({
                      ...f,
                      from_warehouse_id: e.target.value,
                    }))
                  }
                >
                  <option value="">Selecione…</option>
                  {warehousesQuery.data?.data.map((w) => (
                    <option key={w.id} value={w.id}>
                      {w.name}
                    </option>
                  ))}
                </select>
              </div>
              <div className="space-y-2">
                <Label htmlFor="transfer-to">Destino</Label>
                <select
                  id="transfer-to"
                  aria-label="Depósito de destino"
                  className="h-10 w-full rounded-md border px-3 text-sm"
                  value={transferForm.to_warehouse_id}
                  onChange={(e) =>
                    setTransferForm((f) => ({
                      ...f,
                      to_warehouse_id: e.target.value,
                    }))
                  }
                >
                  <option value="">Selecione…</option>
                  {warehousesQuery.data?.data.map((w) => (
                    <option key={w.id} value={w.id}>
                      {w.name}
                    </option>
                  ))}
                </select>
              </div>
              <div className="space-y-2">
                <Label>Quantidade</Label>
                <Input
                  type="number"
                  min={1}
                  value={transferForm.quantity}
                  onChange={(e) =>
                    setTransferForm((f) => ({
                      ...f,
                      quantity: e.target.value,
                    }))
                  }
                />
              </div>
            </div>
            <Button type="submit" disabled={transferMutation.isPending}>
              Transferir
            </Button>
          </form>
        </div>
      ) : null}

      {tab === "importar" ? (
        <div className="max-w-2xl space-y-4 rounded-xl border bg-white p-6">
          <h3 className="font-medium">Importar planilha CSV</h3>
          <p className="text-sm text-stone-600">
            Colunas: <code>sku</code>, <code>warehouse_code</code>,{" "}
            <code>quantity</code>. A quantidade define o saldo final desejado.
          </p>
          <input
            ref={fileInputRef}
            type="file"
            accept=".csv,text/csv"
            className="hidden"
            onChange={async (e) => {
              const file = e.target.files?.[0];
              if (!file) return;
              setCsvText(await file.text());
            }}
          />
          <div className="flex gap-2">
            <Button
              type="button"
              variant="outline"
              onClick={() => fileInputRef.current?.click()}
            >
              Escolher arquivo
            </Button>
            <Button
              type="button"
              onClick={() => importMutation.mutate({ csv: csvText })}
              disabled={!csvText || importMutation.isPending}
            >
              Importar
            </Button>
          </div>
          <Textarea
            rows={10}
            value={csvText}
            onChange={(e) => setCsvText(e.target.value)}
            placeholder="sku,warehouse_code,quantity&#10;BCR-M-ROSA,principal,25"
          />
        </div>
      ) : null}
    </div>
  );
}

function InventoryTable({
  loading,
  error,
  items,
  adjustingId,
  delta,
  onDeltaChange,
  onStartAdjust,
  onConfirmAdjust,
}: {
  loading: boolean;
  error: boolean;
  items: Array<{
    variant_id: string;
    warehouse_id: string;
    product_name: string;
    sku: string;
    size: keyof typeof productSizeLabels;
    color: string;
    warehouse_name: string;
    quantity: number;
    reserved: number;
    available: number;
    is_low_stock: boolean;
  }>;
  adjustingId: string | null;
  delta: string;
  onDeltaChange: (value: string) => void;
  onStartAdjust: (id: string) => void;
  onConfirmAdjust: (variantId: string, warehouseId: string) => void;
}) {
  if (loading) return <p>Carregando…</p>;
  if (error) return <p className="text-red-600">Erro ao carregar estoque.</p>;

  return (
    <div className="overflow-hidden rounded-xl border bg-white">
      <Table>
        <TableHeader>
          <TableRow>
            <TableHead>Produto</TableHead>
            <TableHead>SKU</TableHead>
            <TableHead>Depósito</TableHead>
            <TableHead>Tamanho</TableHead>
            <TableHead>Qtd</TableHead>
            <TableHead>Reservado</TableHead>
            <TableHead>Disponível</TableHead>
            <TableHead>Ajuste</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {items.map((item) => {
            const rowKey = `${item.variant_id}-${item.warehouse_id}`;
            return (
              <TableRow
                key={rowKey}
                className={item.is_low_stock ? "bg-amber-50/60" : undefined}
              >
                <TableCell className="font-medium">
                  {item.product_name}
                </TableCell>
                <TableCell>{item.sku}</TableCell>
                <TableCell>{item.warehouse_name}</TableCell>
                <TableCell>{productSizeLabels[item.size]}</TableCell>
                <TableCell>{item.quantity}</TableCell>
                <TableCell>{item.reserved}</TableCell>
                <TableCell
                  className={
                    item.is_low_stock ? "font-medium text-amber-800" : ""
                  }
                >
                  {item.available}
                </TableCell>
                <TableCell>
                  {adjustingId === rowKey ? (
                    <div className="flex gap-2">
                      <Input
                        type="number"
                        className="h-8 w-20"
                        value={delta}
                        onChange={(e) => onDeltaChange(e.target.value)}
                      />
                      <Button
                        size="sm"
                        onClick={() =>
                          onConfirmAdjust(item.variant_id, item.warehouse_id)
                        }
                      >
                        Confirmar
                      </Button>
                    </div>
                  ) : (
                    <Button
                      size="sm"
                      variant="outline"
                      onClick={() => onStartAdjust(rowKey)}
                    >
                      Ajustar
                    </Button>
                  )}
                </TableCell>
              </TableRow>
            );
          })}
        </TableBody>
      </Table>
    </div>
  );
}
