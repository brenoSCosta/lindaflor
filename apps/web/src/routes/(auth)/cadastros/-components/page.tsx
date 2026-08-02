import { getRouteApi } from "@tanstack/react-router";

import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { ConcessionsTab } from "@/routes/(auth)/cadastros/-components/concessions-tab";
import { InstallationsTab } from "@/routes/(auth)/cadastros/-components/installations-tab";
import { MeasurementEquipmentsTab } from "@/routes/(auth)/cadastros/-components/measurement-equipments-tab";
import {
  cadastrosSearchSchema,
  type CadastrosSearch,
} from "@/routes/(auth)/cadastros/-components/search-schema";
import { TanksTab } from "@/routes/(auth)/cadastros/-components/tanks-tab";

const routeApi = getRouteApi("/(auth)/cadastros/");

export function CadastrosPage() {
  const search = routeApi.useSearch();
  const navigate = routeApi.useNavigate();

  const setTab = (tab: CadastrosSearch["tab"]) => {
    void navigate({ search: (prev) => ({ ...prev, tab }) });
  };

  return (
    <div className="container mx-auto max-w-5xl space-y-6 px-4 py-8">
      <div className="space-y-1">
        <h1 className="text-2xl font-semibold tracking-tight">Cadastros</h1>
        <p className="text-sm text-muted-foreground">
          Dados mestres da organização ativa: tanques, concessões, instalações e
          trenas.
        </p>
      </div>

      <Tabs
        value={search.tab}
        onValueChange={(value) => {
          const nextTab = cadastrosSearchSchema.shape.tab.parse(value);
          setTab(nextTab);
        }}
      >
        <TabsList>
          <TabsTrigger value="tanks">Tanques</TabsTrigger>
          <TabsTrigger value="concessions">Concessões</TabsTrigger>
          <TabsTrigger value="installations">Instalações</TabsTrigger>
          <TabsTrigger value="trenas">Trenas</TabsTrigger>
        </TabsList>
        <TabsContent value="tanks">
          <TanksTab />
        </TabsContent>
        <TabsContent value="concessions">
          <ConcessionsTab />
        </TabsContent>
        <TabsContent value="installations">
          <InstallationsTab />
        </TabsContent>
        <TabsContent value="trenas">
          <MeasurementEquipmentsTab />
        </TabsContent>
      </Tabs>
    </div>
  );
}
