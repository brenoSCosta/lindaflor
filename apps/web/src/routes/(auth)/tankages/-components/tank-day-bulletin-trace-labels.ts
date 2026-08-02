import type { TankDayBulletinAuditEventOutput } from "@lindaflor/shared/schemas/tankage/day-bulletins";

const FIELD_LABELS: Record<string, string> = {
  measured_at: "Hora",
  current_measurement: "Altura",
  oil_temperature_c: "Temp. óleo",
  ambient_temperature_c: "Temp. ambiente",
  observation: "Observação",
  operator_user_id: "Operador",
  measurement_equipment_id: "Trena",
  transferred_at: "Hora",
  height_before_m: "Altura antes",
  height_after_m: "Altura depois",
  destination_label: "Destino",
};

function formatAuditScalar(value: string | number | null): string {
  if (value == null) return "—";
  if (typeof value === "number") {
    return value.toLocaleString("pt-BR", {
      maximumFractionDigits: 3,
    });
  }
  if (/^\d{4}-\d{2}-\d{2}T/.test(value)) {
    const date = new Date(value);
    if (!Number.isNaN(date.getTime())) {
      return date.toLocaleString("pt-BR", {
        day: "2-digit",
        month: "2-digit",
        year: "numeric",
        hour: "2-digit",
        minute: "2-digit",
      });
    }
  }
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : "—";
}

export function buildBulletinTraceLabel(
  event: Pick<
    TankDayBulletinAuditEventOutput,
    "entity_type" | "action" | "actor_name"
  >,
): string {
  const by = event.actor_name.trim();
  const bySuffix = by.length > 0 ? ` por ${by}` : "";

  if (event.entity_type === "tank_day_bulletin") {
    if (event.action === "approve") {
      return by ? `Aprovado por ${by}` : "Aprovado (supervisão)";
    }
    if (event.action === "reopen") {
      return by ? `Reaberto por ${by}` : "Reaberto (administração)";
    }
    if (event.action === "delete") {
      return `Operações do dia excluídas${bySuffix}`;
    }
  }

  if (event.entity_type === "tankage") {
    if (event.action === "create")
      return `Medição de produção criada${bySuffix}`;
    if (event.action === "update") return `Medição atualizada${bySuffix}`;
    if (event.action === "retreat") return `Retratamento de medição${bySuffix}`;
    if (event.action === "delete") return `Medição excluída${bySuffix}`;
  }

  if (event.entity_type === "tank_transfer") {
    if (event.action === "create") return `Transferência registrada${bySuffix}`;
    if (event.action === "retreat")
      return `Retratamento de transferência${bySuffix}`;
    if (event.action === "delete") return `Transferência excluída${bySuffix}`;
  }

  return `Ação registrada${bySuffix}`;
}

export function buildBulletinTraceDetail(
  event: Pick<TankDayBulletinAuditEventOutput, "entity_type" | "metadata">,
): string | null {
  const metadata = event.metadata;
  if (metadata == null) return null;

  if (metadata.changes != null && metadata.changes.length > 0) {
    const changesDetail = metadata.changes
      .map((change) => {
        const fieldLabel = FIELD_LABELS[change.field] ?? change.field;
        const unit =
          change.field === "current_measurement" ||
          change.field === "height_before_m" ||
          change.field === "height_after_m"
            ? " m"
            : change.field === "oil_temperature_c" ||
                change.field === "ambient_temperature_c"
              ? " °C"
              : "";
        return `${fieldLabel} ${formatAuditScalar(change.from)}${unit} → ${formatAuditScalar(change.to)}${unit}`;
      })
      .join(" · ");
    if (
      metadata.justification != null &&
      metadata.justification.trim().length > 0
    ) {
      return `${changesDetail} · Justificativa: ${metadata.justification.trim()}`;
    }
    return changesDetail;
  }

  if (
    event.entity_type === "tank_day_bulletin" &&
    metadata.deleted_count != null
  ) {
    const count = metadata.deleted_count;
    return count === 1 ? "1 medição excluída" : `${count} medições excluídas`;
  }

  if (event.entity_type === "tankage") {
    if (metadata.current_measurement != null) {
      return `Altura ${formatAuditScalar(metadata.current_measurement)} m`;
    }
  }

  if (event.entity_type === "tank_transfer") {
    const parts: string[] = [];
    if (metadata.height_before_m != null && metadata.height_after_m != null) {
      parts.push(
        `Altura ${formatAuditScalar(metadata.height_before_m)} m → ${formatAuditScalar(metadata.height_after_m)} m`,
      );
    }
    if (metadata.gross_volume_out_m3 != null) {
      parts.push(`${formatAuditScalar(metadata.gross_volume_out_m3)} m³`);
    }
    if (
      metadata.destination_label != null &&
      metadata.destination_label.trim().length > 0
    ) {
      parts.push(`para ${metadata.destination_label.trim()}`);
    }
    return parts.length > 0 ? parts.join(" · ") : null;
  }

  return null;
}
