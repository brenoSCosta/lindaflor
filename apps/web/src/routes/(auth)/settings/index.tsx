import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { z } from "zod";

import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { useIsMobile } from "@/hooks/use-mobile";
import { AccountCard } from "@/routes/(auth)/settings/-components/account-card";
import { DangerZoneCard } from "@/routes/(auth)/settings/-components/danger-zone-card";
import { LinkedAccountsCard } from "@/routes/(auth)/settings/-components/linked-accounts-card";
import { ProfileCard } from "@/routes/(auth)/settings/-components/profile-card";
import { SessionsCard } from "@/routes/(auth)/settings/-components/sessions-card";
import { TwoFactorCard } from "@/routes/(auth)/settings/-components/two-factor-card";

export const tabSchema = z.enum([
  "profile",
  "account",
  "sessions",
  "security",
  "linked-accounts",
  "danger",
]);

const settingsSearchSchema = z.object({
  tab: tabSchema.default("profile"),
});

export const Route = createFileRoute("/(auth)/settings/")({
  validateSearch: (search: Record<string, unknown>) =>
    settingsSearchSchema.parse(search),
  component: SettingsPage,
});

function SettingsPage() {
  const { tab } = Route.useSearch();
  const navigate = useNavigate({ from: Route.fullPath });
  const isMobile = useIsMobile();

  const handleTabChange = (value: string | null) => {
    if (value === null) return;
    void navigate({
      search: { tab: tabSchema.parse(value) },
    });
  };

  return (
    <div className="container max-w-4xl mx-auto py-8 px-4 space-y-6">
      <div className="space-y-1">
        <h1 className="text-2xl font-semibold tracking-tight">Configurações</h1>
        <p className="text-sm text-muted-foreground">
          Gerencie seu perfil, conta, sessões, segurança, contas vinculadas e
          exclusão de conta.
        </p>
      </div>

      <Tabs
        value={tab}
        onValueChange={handleTabChange}
        className="flex flex-col"
      >
        {isMobile ? (
          <Select value={tab} onValueChange={handleTabChange}>
            <SelectTrigger
              aria-label="Seção de configurações"
              className="w-full"
            >
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="profile">Perfil</SelectItem>
              <SelectItem value="account">Conta</SelectItem>
              <SelectItem value="sessions">Sessões</SelectItem>
              <SelectItem value="security">Segurança</SelectItem>
              <SelectItem value="linked-accounts">Contas vinculadas</SelectItem>
              <SelectItem value="danger">Zona de perigo</SelectItem>
            </SelectContent>
          </Select>
        ) : (
          <TabsList className="w-full">
            <TabsTrigger value="profile">Perfil</TabsTrigger>
            <TabsTrigger value="account">Conta</TabsTrigger>
            <TabsTrigger value="sessions">Sessões</TabsTrigger>
            <TabsTrigger value="security">Segurança</TabsTrigger>
            <TabsTrigger value="linked-accounts">Contas vinculadas</TabsTrigger>
            <TabsTrigger value="danger">Zona de perigo</TabsTrigger>
          </TabsList>
        )}

        <TabsContent value="profile">
          <ProfileCard />
        </TabsContent>
        <TabsContent value="account">
          <AccountCard />
        </TabsContent>
        <TabsContent value="sessions">
          <SessionsCard />
        </TabsContent>
        <TabsContent value="security">
          <TwoFactorCard />
        </TabsContent>
        <TabsContent value="linked-accounts">
          <LinkedAccountsCard />
        </TabsContent>
        <TabsContent value="danger">
          <DangerZoneCard />
        </TabsContent>
      </Tabs>
    </div>
  );
}
