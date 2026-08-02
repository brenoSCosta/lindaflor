import { Badge } from "@/components/ui/badge";

export function TankCalibrationStatusBadge({
  validUntil,
}: {
  /** `undefined` = sem certificado vigente; `null` = vigente sem data fim. */
  validUntil: string | null | undefined;
}) {
  if (validUntil === undefined) {
    return (
      <Badge variant="outline" className="font-normal">
        Sem arqueação vigente
      </Badge>
    );
  }

  if (validUntil == null) {
    return (
      <Badge variant="secondary" className="font-normal">
        Vigente (sem fim)
      </Badge>
    );
  }

  return (
    <Badge variant="secondary" className="font-normal">
      Vence em {validUntil}
    </Badge>
  );
}
