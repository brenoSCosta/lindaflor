import type { TankCalibrationListItem } from "@lindaflor/shared/schemas/tankage/calibrations";
import { Loader2, Plus, Search, X } from "lucide-react";
import React from "react";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  InputGroup,
  InputGroupAddon,
  InputGroupButton,
  InputGroupInput,
} from "@/components/ui/input-group";
import { Can } from "@/lib/ability";
import { cn, todayIsoDate } from "@/lib/utils";
import {
  certificateDisplayBadge,
  formatValidityRange,
} from "@/routes/(auth)/arqueacao/-components/calibration-status";

export function CertificateHistorySidebar({
  list,
  listPending,
  selectedId,
  creating,
  onSelect,
  onCreate,
}: {
  list: TankCalibrationListItem[];
  listPending: boolean;
  selectedId: string | null;
  creating: boolean;
  onSelect: (id: string) => void;
  onCreate: () => void;
}) {
  const [certificateFilter, setCertificateFilter] = React.useState("");
  const today = todayIsoDate();

  const filteredList = list.filter((item) => {
    const query = certificateFilter.trim().toLowerCase();
    if (query === "") return true;
    return item.certificate_number.toLowerCase().includes(query);
  });

  return (
    <aside className="flex min-h-0 flex-col gap-3 overflow-hidden rounded-xl bg-card p-4 ring-1 ring-foreground/10">
      <div className="flex items-center justify-between gap-2">
        <p className="text-xs font-semibold tracking-wide text-muted-foreground uppercase">
          Histórico de certificados
        </p>
        <Can I="create" a="TankCalibrations">
          <Button
            size="icon-xs"
            variant="outline"
            onClick={onCreate}
            aria-label="Novo certificado"
          >
            <Plus className="size-3.5" />
          </Button>
        </Can>
      </div>

      <InputGroup>
        <InputGroupAddon>
          <Search className="size-3.5" />
        </InputGroupAddon>
        <InputGroupInput
          value={certificateFilter}
          onChange={(e) => setCertificateFilter(e.target.value)}
          placeholder="Pesquisar certificados…"
          aria-label="Pesquisar certificados"
        />
        {certificateFilter ? (
          <InputGroupAddon align="inline-end">
            <InputGroupButton
              size="icon-xs"
              onClick={() => setCertificateFilter("")}
              aria-label="Limpar busca"
            >
              <X className="size-3.5" />
            </InputGroupButton>
          </InputGroupAddon>
        ) : null}
      </InputGroup>

      <div className="min-h-0 flex-1 overflow-auto">
        {listPending ? (
          <Loader2 className="mx-auto size-4 animate-spin text-muted-foreground" />
        ) : list.length === 0 ? (
          <p className="text-sm text-muted-foreground">Nenhum certificado</p>
        ) : filteredList.length === 0 ? (
          <p className="text-sm text-muted-foreground">
            Nenhum certificado encontrado
          </p>
        ) : (
          <ul className="space-y-2">
            {filteredList.map((item) => {
              const badge = certificateDisplayBadge(item, today);
              const selected = selectedId === item.id && !creating;
              return (
                <li key={item.id}>
                  <button
                    type="button"
                    className={cn(
                      "w-full rounded-lg border border-transparent px-3 py-2.5 text-left text-sm transition-colors",
                      selected
                        ? "border-l-2 border-l-primary bg-muted ring-1 ring-foreground/10"
                        : "hover:bg-muted/60",
                    )}
                    onClick={() => onSelect(item.id)}
                  >
                    <div className="flex items-start justify-between gap-2">
                      <span className="truncate font-medium">
                        {item.certificate_number}
                      </span>
                      <Badge
                        variant={badge.variant}
                        className={cn("shrink-0 font-normal", badge.className)}
                      >
                        {badge.label}
                      </Badge>
                    </div>
                    <p className="mt-1.5 text-xs text-muted-foreground">
                      Emissão: {item.issued_at ?? "—"}
                    </p>
                    <p className="text-xs text-muted-foreground">
                      Vigência:{" "}
                      {formatValidityRange(item.valid_from, item.valid_until)}
                    </p>
                    <p className="mt-0.5 text-xs text-muted-foreground">
                      {item.points_count} pontos
                    </p>
                  </button>
                </li>
              );
            })}
          </ul>
        )}
      </div>
    </aside>
  );
}
