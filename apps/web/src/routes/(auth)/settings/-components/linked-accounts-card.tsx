import { useQuery } from "@tanstack/react-query";
import { format } from "date-fns";
import { KeyRound, Link2 } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";

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
import { authClient } from "@/lib/auth-client";

type SocialProvider = "google";

const socialProviders: { id: SocialProvider; label: string }[] = [
  { id: "google", label: "Google" },
];

const providerLabel = (providerId: string): string => {
  if (providerId === "credential") return "E-mail e senha";
  const found = socialProviders.find((p) => p.id === providerId);
  return found?.label ?? providerId;
};

export function LinkedAccountsCard() {
  const [pendingProvider, setPendingProvider] = useState<SocialProvider | null>(
    null,
  );
  const [unlinking, setUnlinking] = useState<string | null>(null);

  const {
    data: accounts,
    isPending,
    refetch,
  } = useQuery({
    queryKey: ["linked-accounts"],
    queryFn: async () => {
      const result = await authClient.listAccounts();
      return result.data ?? [];
    },
  });

  const linkedProviderIds = new Set(accounts?.map((a) => a.providerId) ?? []);
  const availableProviders = socialProviders.filter(
    (p) => !linkedProviderIds.has(p.id),
  );
  const canUnlink = (accounts?.length ?? 0) > 1;

  const handleLink = async (provider: SocialProvider) => {
    setPendingProvider(provider);
    await authClient.linkSocial(
      {
        provider,
        callbackURL: `${window.location.origin}/settings?tab=linked-accounts`,
      },
      {
        onError: (error) => {
          toast.error(error.error.message || error.error.statusText);
          setPendingProvider(null);
        },
      },
    );
  };

  const handleUnlink = async (providerId: string, accountId: string) => {
    setUnlinking(accountId);
    await authClient.unlinkAccount(
      { providerId, accountId },
      {
        onSuccess: async () => {
          await refetch();
          toast.success(`${providerLabel(providerId)} desvinculado`);
          setUnlinking(null);
        },
        onError: (error) => {
          toast.error(error.error.message || error.error.statusText);
          setUnlinking(null);
        },
      },
    );
  };

  return (
    <Card>
      <CardHeader>
        <CardTitle>Contas vinculadas</CardTitle>
        <CardDescription>
          Métodos de login vinculados à sua conta.
        </CardDescription>
      </CardHeader>
      <CardContent>
        {isPending ? (
          <div className="space-y-2">
            <Skeleton className="h-14 w-full" />
            <Skeleton className="h-14 w-full" />
          </div>
        ) : !accounts || accounts.length === 0 ? (
          <Empty>
            <EmptyMedia variant="icon">
              <KeyRound className="size-8" />
            </EmptyMedia>
            <EmptyTitle>Nenhuma conta vinculada</EmptyTitle>
            <EmptyDescription>
              Conecte um método de login abaixo para começar.
            </EmptyDescription>
          </Empty>
        ) : (
          <ItemGroup>
            {accounts.map((account) => (
              <Item key={account.id} variant="outline">
                <ItemMedia variant="icon">
                  <Link2 />
                </ItemMedia>
                <ItemContent>
                  <ItemTitle>{providerLabel(account.providerId)}</ItemTitle>
                  <ItemDescription>
                    Vinculado em {format(new Date(account.createdAt), "PPp")}
                  </ItemDescription>
                </ItemContent>
                <ItemActions>
                  <Button
                    variant="ghost"
                    size="sm"
                    disabled={!canUnlink || unlinking === account.id}
                    onClick={() =>
                      handleUnlink(account.providerId, account.accountId)
                    }
                  >
                    {unlinking === account.id
                      ? "Desvinculando…"
                      : "Desvincular"}
                  </Button>
                </ItemActions>
              </Item>
            ))}
          </ItemGroup>
        )}
      </CardContent>
      {availableProviders.length > 0 && (
        <CardFooter className="flex flex-wrap gap-2">
          {availableProviders.map((p) => (
            <Button
              key={p.id}
              variant="outline"
              size="sm"
              disabled={pendingProvider !== null}
              onClick={() => handleLink(p.id)}
            >
              {pendingProvider === p.id
                ? `Redirecionando para ${p.label}…`
                : `Conectar ${p.label}`}
            </Button>
          ))}
        </CardFooter>
      )}
    </Card>
  );
}
