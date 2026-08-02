import { SUBJECT_ACTIONS } from "@lindaflor/shared/lib/ability/actions-by-subject";
import {
  abilityCan,
  type ActionsBySubject,
} from "@lindaflor/shared/lib/ability/subjects";
import { toOrgRole } from "@lindaflor/shared/lib/roles";
import { Check, Shield, X } from "lucide-react";

import { Badge } from "@/components/ui/badge";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { useAppAbility } from "@/lib/ability";
import { authClient } from "@/lib/auth-client";
import { cn } from "@/lib/utils";

type AllPossibleActions = {
  [K in keyof ActionsBySubject]: ActionsBySubject[K];
}[keyof ActionsBySubject];

const ALL_ACTIONS = [
  "create",
  "read",
  "update",
  "delete",
  "manage",
  "ban",
  "impersonate",
  "progress",
  "certificate",
  "approve",
  "reopen",
  "retreat",
] as const satisfies readonly AllPossibleActions[];

// `Record<AllPossibleActions, string>` errors if an action from ActionsBySubject
// is missing from ACTION_LABELS (and thus from ALL_ACTIONS).
const ACTION_LABELS: Record<AllPossibleActions, string> = {
  create: "Criar",
  read: "Ler",
  update: "Atualizar",
  delete: "Excluir",
  manage: "Gerenciar",
  ban: "Banir",
  impersonate: "Atuar como",
  progress: "Progresso",
  certificate: "Certificado",
  approve: "Aprovar",
  reopen: "Reabrir",
  retreat: "Retratar",
};

const BASE_RESOURCES = [
  // { name: "Todo" as const, label: "Tarefas" },
  { name: "Member" as const, label: "Membros" },
  { name: "Concessions" as const, label: "Concessões" },
  { name: "Installations" as const, label: "Instalações" },
  { name: "MeasurementEquipments" as const, label: "Trenas" },
  { name: "LabOilAnalyses" as const, label: "Análises de laboratório" },
  { name: "Tanks" as const, label: "Tanques" },
  { name: "TankCalibrations" as const, label: "Arqueações" },
  { name: "Tankages" as const, label: "Tancagens" },
  { name: "TankTransfers" as const, label: "Transferências" },
  { name: "TankDayBulletins" as const, label: "Boletins diários" },
  { name: "Curriculum" as const, label: "Currículo" },
  { name: "Training" as const, label: "Cursos de treinamento" },
  { name: "TrainingEnrollment" as const, label: "Inscrições em treinamentos" },
];
const ADMIN_RESOURCE = { name: "User" as const, label: "Usuários (admin)" };

type Resource = (typeof BASE_RESOURCES)[number] | typeof ADMIN_RESOURCE;

type AnyAction = (typeof ALL_ACTIONS)[number];

type ValidAction<K extends keyof ActionsBySubject> =
  (typeof SUBJECT_ACTIONS)[K][number];

const isValidAction = <K extends keyof ActionsBySubject>(
  resource: K,
  action: AnyAction,
): action is ValidAction<K> => {
  const valid: ReadonlyArray<string> = SUBJECT_ACTIONS[resource];
  return valid.includes(action);
};

export function YourAccessCard() {
  const { data: session } = authClient.useSession();
  const { data: activeOrganization } = authClient.useActiveOrganization();
  const { data: activeMember } = authClient.useActiveMember();
  const ability = useAppAbility();

  const isAdmin = session?.user.role?.includes("admin") ?? false;
  const isModerator = session?.user.role?.includes("moderator") ?? false;
  const orgRole = toOrgRole(activeMember?.role);
  const resources: Resource[] = isAdmin
    ? [...BASE_RESOURCES, ADMIN_RESOURCE]
    : BASE_RESOURCES;

  return (
    <Card>
      <CardHeader>
        <div className="flex items-center gap-2">
          <Shield className="size-5 text-muted-foreground" />
          <CardTitle>Seu acesso</CardTitle>
        </div>
        <CardDescription>
          O que você pode fazer com base no seu papel
          {activeOrganization ? ` em ${activeOrganization.name}` : ""}.
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-6">
        <div className="flex flex-wrap items-center gap-2">
          <span className="text-sm text-muted-foreground">Conectado como</span>
          <span className="text-sm font-medium">{session?.user.name}</span>
          {isAdmin && <Badge variant="destructive">Admin da plataforma</Badge>}
          {isModerator && <Badge variant="secondary">Moderador</Badge>}
          {orgRole === "owner" && <Badge>Proprietário da org</Badge>}
          {orgRole === "admin" && (
            <Badge variant="secondary">Admin da org</Badge>
          )}
          {orgRole === "operator" && (
            <Badge variant="secondary">Operador</Badge>
          )}
          {orgRole === "supervisor" && (
            <Badge variant="secondary">Supervisor</Badge>
          )}
          {orgRole === "member" && <Badge variant="outline">Membro</Badge>}
          {!isAdmin && !orgRole && (
            <Badge variant="outline">Sem papel na org</Badge>
          )}
        </div>

        {isAdmin && (
          <p className="text-sm text-muted-foreground">
            Admins da plataforma gerenciam Usuários em toda a plataforma. Dados
            com escopo de organização (Tarefas, Membros, Concessões,
            Instalações, Tancagens) seguem seu papel na organização ativa. Para
            atuar em uma organização à qual você não pertence, atue como um
            membro pelo painel administrativo.
          </p>
        )}

        <div className="overflow-x-auto rounded-md border">
          <Table className="min-w-160">
            <TableHeader>
              <TableRow>
                <TableHead className="w-40">Recurso</TableHead>
                {ALL_ACTIONS.map((action) => (
                  <TableHead key={action} className="capitalize">
                    {ACTION_LABELS[action]}
                  </TableHead>
                ))}
              </TableRow>
            </TableHeader>
            <TableBody>
              {resources.map((resource) => (
                <TableRow key={resource.name}>
                  <TableCell className="font-medium">
                    {resource.label}
                  </TableCell>
                  {ALL_ACTIONS.map((action) => {
                    const applicable = isValidAction(resource.name, action);
                    const allowed = applicable
                      ? abilityCan(ability, action, resource.name)
                      : null;
                    return (
                      <TableCell key={action}>
                        {allowed === null ? (
                          <span
                            className="text-muted-foreground/30"
                            aria-label="Não aplicável"
                          >
                            N/A
                          </span>
                        ) : allowed ? (
                          <Check
                            className={cn("size-4 text-success")}
                            aria-label="Permitido"
                          />
                        ) : (
                          <X
                            className="size-4 text-muted-foreground/40"
                            aria-label="Negado"
                          />
                        )}
                      </TableCell>
                    );
                  })}
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </div>
      </CardContent>
    </Card>
  );
}
