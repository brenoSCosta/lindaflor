import {
  BookOpen,
  Crop,
  Database,
  FileText,
  Fuel,
  GraduationCap,
  LayoutDashboard,
  Map,
  Ruler,
  Settings2,
  Shield,
  ShieldCheck,
  SquareTerminal,
  Table,
  Upload,
  Users,
  Workflow,
} from "lucide-react";

import {
  NavFlatGroup,
  NavMain,
  type NavFlatLink,
  type NavMainItem,
} from "@/components/nav-main";
import { NavUser } from "@/components/nav-user";
import { OrgSwitcher } from "@/components/org-switcher";
import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarHeader,
  SidebarRail,
} from "@/components/ui/sidebar";
import { useAppAbility } from "@/lib/ability";
import { authClient } from "@/lib/auth-client";

export function AppSidebar() {
  const { data: session } = authClient.useSession();
  const { data: activeOrganization } = authClient.useActiveOrganization();
  const ability = useAppAbility();

  const hasSession = !!session;
  const hasOrg = hasSession && !!activeOrganization;
  const isAdmin = hasSession && !!session.user.role?.includes("admin");
  const canReadTraining = hasOrg && ability.can("read", "Training");
  const canManageTraining = hasOrg && ability.can("manage", "Training");
  const devRouteVisible = import.meta.env.DEV;

  const geralLinks: NavFlatLink[] = [
    { title: "Início", to: "/", icon: LayoutDashboard },
    { title: "Painel", to: "/dashboard", icon: LayoutDashboard },
    ...(hasOrg
      ? [{ title: "Tanques", to: "/tankages" as const, icon: Fuel }]
      : []),
    ...(hasOrg && ability.can("read", "TankCalibrations")
      ? [{ title: "Arqueação", to: "/arqueacao" as const, icon: Ruler }]
      : []),
    ...(hasOrg && ability.can("manage", "Tanks")
      ? [{ title: "Cadastros", to: "/cadastros" as const, icon: Database }]
      : []),
  ];

  const trainingItems = [
    ...(canReadTraining
      ? [
          {
            title: "Catálogo",
            to: "/training" as const,
            icon: GraduationCap,
          },
        ]
      : []),
    ...(canManageTraining
      ? [
          {
            title: "Gerenciar",
            to: "/training/manage" as const,
            icon: Settings2,
          },
        ]
      : []),
  ];

  const platformItems: NavMainItem[] = [
    ...(trainingItems.length > 0
      ? [
          {
            title: "Treinamentos",
            icon: GraduationCap,
            items: trainingItems,
          } satisfies NavMainItem,
        ]
      : []),
    ...(hasSession && ability.can("manage", "Curriculum")
      ? [
          {
            title: "Banco de talentos",
            icon: Users,
            to: "/curriculum" as const,
          } satisfies NavMainItem,
        ]
      : []),
    ...(isAdmin
      ? [
          {
            title: "Administração",
            icon: Shield,
            items: [
              {
                title: "Permissões",
                to: "/permissions" as const,
                icon: ShieldCheck,
              },
              { title: "Usuários", to: "/admin" as const, icon: Shield },
            ],
          } satisfies NavMainItem,
        ]
      : []),
  ];

  const devItems: NavMainItem[] = devRouteVisible
    ? [
        {
          title: "Componentes",
          icon: SquareTerminal,
          items: [
            {
              title: "Upload de arquivos",
              to: "/dev/component-examples/file-upload" as const,
              icon: Upload,
            },
            {
              title: "Recorte de imagem",
              to: "/dev/component-examples/image-cropper" as const,
              icon: Crop,
            },
            {
              title: "Tour guiado",
              to: "/dev/component-examples/tour" as const,
              icon: Map,
            },
            {
              title: "React Flow",
              to: "/dev/component-examples/react-flow" as const,
              icon: Workflow,
            },
            {
              title: "Data table V1",
              to: "/dev/todos" as const,
              icon: Table,
            },
            {
              title: "Data Table V2",
              to: "/dev/component-examples/data-table" as const,
              icon: Table,
            },
            {
              title: "Markdown",
              to: "/dev/component-examples/markdown" as const,
              icon: FileText,
            },
          ],
        } satisfies NavMainItem,
        {
          title: "Documentação",
          icon: BookOpen,
          items: [
            {
              title: "CPFO",
              to: "/dev/docs/controle-producao-fiscal-operacional/$slug" as const,
              params: { slug: "intro" },
              icon: FileText,
            },
          ],
        } satisfies NavMainItem,
      ]
    : [];

  if (!session) return null;

  return (
    <Sidebar collapsible="icon">
      <SidebarHeader>
        <OrgSwitcher />
      </SidebarHeader>
      <SidebarContent>
        <NavFlatGroup label="Geral" links={geralLinks} />
        <NavMain items={platformItems} label="Plataforma" />
        <NavMain items={devItems} label="Desenvolvimento" />
      </SidebarContent>
      <SidebarFooter>
        <NavUser />
      </SidebarFooter>
      <SidebarRail />
    </Sidebar>
  );
}
