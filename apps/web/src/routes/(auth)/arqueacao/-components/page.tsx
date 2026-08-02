import { useQuery } from "@tanstack/react-query";
import { getRouteApi } from "@tanstack/react-router";
import { Loader2 } from "lucide-react";
import React from "react";

import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { orpc } from "@/lib/orpc";
import { arqueacaoSearchSchema } from "@/routes/(auth)/arqueacao/-components/search-schema";
import { TankCalibrationPanel } from "@/routes/(auth)/arqueacao/-components/tank-calibration-panel";

const routeApi = getRouteApi("/(auth)/arqueacao/");

export function ArqueacaoPage() {
  const search = routeApi.useSearch();
  const navigate = routeApi.useNavigate();

  const {
    data: tanks = [],
    isPending,
    isError,
    error,
  } = useQuery(
    orpc.tanks.v1.tank.list.all.queryOptions({
      select: (result) => result.data,
    }),
  );

  const selectedTank = tanks.find((tank) => tank.id === search.tank_id) ?? null;

  const tankItems = React.useMemo(
    () =>
      Object.fromEntries(
        tanks.map((tank) => [
          tank.id,
          `${tank.tag} — ${tank.installation_name}`,
        ]),
      ),
    [tanks],
  );

  const setTankId = (tankId: string | null) => {
    void navigate({
      search: (prev) =>
        arqueacaoSearchSchema.parse({
          ...prev,
          tank_id: tankId ?? undefined,
        }),
    });
  };

  return (
    <div className="mx-auto flex w-full max-w-7xl flex-col gap-6 px-4 py-8 sm:px-6 lg:px-8">
      <div className="flex flex-col gap-4 sm:flex-row sm:items-end sm:justify-between">
        <div className="space-y-1">
          <h1 className="text-2xl font-semibold tracking-tight">Arqueação</h1>
          <p className="text-sm text-muted-foreground">
            Certificados e tabela altura → volume por tanque.
          </p>
        </div>

        <div className="space-y-1.5">
          <Label htmlFor="arqueacao-tank">Tanque</Label>
          {isPending ? (
            <div className="flex h-9 w-72 items-center gap-2 text-sm text-muted-foreground">
              <Loader2 className="size-4 animate-spin" />
              Carregando tanques…
            </div>
          ) : isError ? (
            <p className="text-sm text-destructive">
              {error?.message ?? "Falha ao carregar tanques"}
            </p>
          ) : tanks.length === 0 ? (
            <p className="text-sm text-muted-foreground">
              Nenhum tanque cadastrado. Cadastre em Cadastros → Tanques.
            </p>
          ) : (
            <Select
              items={tankItems}
              value={search.tank_id ?? null}
              onValueChange={(value) => {
                setTankId(typeof value === "string" ? value : null);
              }}
            >
              <SelectTrigger id="arqueacao-tank" className="w-full sm:w-80">
                <SelectValue placeholder="Selecione um tanque" />
              </SelectTrigger>
              <SelectContent>
                {tanks.map((tank) => (
                  <SelectItem key={tank.id} value={tank.id}>
                    {`${tank.tag} — ${tank.installation_name}`}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          )}
        </div>
      </div>

      {search.tank_id != null && selectedTank == null && !isPending ? (
        <p className="text-sm text-destructive">
          Tanque não encontrado na organização ativa.
        </p>
      ) : null}

      {selectedTank != null ? (
        <TankCalibrationPanel key={selectedTank.id} tank={selectedTank} />
      ) : tanks.length > 0 && !isPending ? (
        <div className="rounded-xl border border-dashed bg-muted/30 px-6 py-16 text-center text-sm text-muted-foreground">
          Selecione um tanque para ver ou editar a arqueação.
        </div>
      ) : null}
    </div>
  );
}
