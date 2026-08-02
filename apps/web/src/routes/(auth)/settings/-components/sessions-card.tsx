import { ORPCError } from "@orpc/client";
import { useQuery } from "@tanstack/react-query";
import { format } from "date-fns";
import { Monitor } from "lucide-react";
import { toast } from "sonner";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import {
  Empty,
  EmptyDescription,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty";
import {
  Item,
  ItemActions,
  ItemContent,
  ItemDescription,
  ItemGroup,
  ItemMedia,
  ItemTitle,
} from "@/components/ui/item";
import { Skeleton } from "@/components/ui/skeleton";
import { useTimezone } from "@/context/timezone";
import { authClient } from "@/lib/auth-client";

type Session = NonNullable<typeof authClient.$Infer.Session>["session"];

export function SessionsCard() {
  const { timezone } = useTimezone();
  const { data: currentSessionData } = authClient.useSession();

  const {
    data: sessions,
    isPending,
    refetch,
  } = useQuery<Session[]>({
    queryKey: ["sessions"],
    queryFn: async () => {
      const result = await authClient.listSessions({
        fetchOptions: {
          onError: (e) => {
            throw new ORPCError("ERROR", {
              message:
                e.error.message ??
                e.error.statusText ??
                "Falha ao carregar sessões",
            });
          },
        },
      });
      return result.data ?? [];
    },
  });

  const currentSessionId = currentSessionData?.session.id;
  const otherSessionsCount =
    sessions?.filter((s) => s.id !== currentSessionId).length ?? 0;

  const revokeOne = async (token: string) => {
    await authClient.revokeSession(
      { token },
      {
        onSuccess: async () => {
          await refetch();
          toast.success("Sessão revogada");
        },
        onError: (e) => {
          throw new ORPCError("ERROR", {
            message:
              e.error.message ??
              e.error.statusText ??
              "Falha ao revogar a sessão",
          });
        },
      },
    );
  };

  const revokeOthers = async () => {
    await authClient.revokeOtherSessions(
      {},
      {
        onSuccess: async () => {
          await refetch();
          toast.success("Saída efetuada de todas as outras sessões");
        },
        onError: (e) => {
          throw new ORPCError("ERROR", {
            message:
              e.error.message ??
              e.error.statusText ??
              "Falha ao sair das outras sessões",
          });
        },
      },
    );
  };

  return (
    <Card>
      <CardHeader>
        <CardTitle>Sessões ativas</CardTitle>
        <CardDescription>
          Dispositivos atualmente conectados à sua conta.
        </CardDescription>
      </CardHeader>
      <CardContent>
        {isPending ? (
          <div className="space-y-2">
            <Skeleton className="h-14 w-full" />
            <Skeleton className="h-14 w-full" />
          </div>
        ) : !sessions || sessions.length === 0 ? (
          <Empty>
            <EmptyMedia variant="icon">
              <Monitor className="size-8" />
            </EmptyMedia>
            <EmptyTitle>Nenhuma sessão ativa</EmptyTitle>
            <EmptyDescription>
              Você não tem nenhuma sessão para gerenciar.
            </EmptyDescription>
          </Empty>
        ) : (
          <ItemGroup>
            {sessions.map((s) => {
              const isCurrent = s.id === currentSessionId;
              return (
                <Item key={s.id} variant="outline">
                  <ItemMedia variant="icon">
                    <Monitor />
                  </ItemMedia>
                  <ItemContent>
                    <ItemTitle>
                      {s.userAgent ?? "Dispositivo desconhecido"}
                      {isCurrent && (
                        <Badge variant="secondary" className="ml-2">
                          Este dispositivo
                        </Badge>
                      )}
                    </ItemTitle>
                    <ItemDescription>
                      {s.ipAddress ?? "IP desconhecido"} &middot; Conectado em{" "}
                      {format(new Date(s.createdAt), "PPp")} ({timezone})
                    </ItemDescription>
                  </ItemContent>
                  <ItemActions>
                    <Button
                      variant="ghost"
                      size="sm"
                      disabled={isCurrent}
                      onClick={() => revokeOne(s.token)}
                    >
                      Revogar
                    </Button>
                  </ItemActions>
                </Item>
              );
            })}
          </ItemGroup>
        )}
      </CardContent>
      <CardFooter>
        <Button
          variant="outline"
          size="sm"
          disabled={isPending || otherSessionsCount === 0}
          onClick={revokeOthers}
        >
          Sair de todas as outras sessões
          {otherSessionsCount > 0 && ` (${otherSessionsCount})`}
        </Button>
      </CardFooter>
    </Card>
  );
}
