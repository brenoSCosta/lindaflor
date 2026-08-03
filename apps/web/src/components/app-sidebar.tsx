import { LayoutDashboard } from "lucide-react";

import { NavFlatGroup, type NavFlatLink } from "@/components/nav-main";
import { NavUser } from "@/components/nav-user";
import { OrgSwitcher } from "@/components/org-switcher";
import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarHeader,
  SidebarRail,
} from "@/components/ui/sidebar";
import { authClient } from "@/lib/auth-client";

export function AppSidebar() {
  const { data: session } = authClient.useSession();

  const geralLinks: NavFlatLink[] = [
    { title: "Início", to: "/", icon: LayoutDashboard },
    { title: "Painel", to: "/dashboard", icon: LayoutDashboard },
  ];

  if (!session) return null;

  return (
    <Sidebar collapsible="icon">
      <SidebarHeader>
        <OrgSwitcher />
      </SidebarHeader>
      <SidebarContent>
        <NavFlatGroup label="Geral" links={geralLinks} />
      </SidebarContent>
      <SidebarFooter>
        <NavUser />
      </SidebarFooter>
      <SidebarRail />
    </Sidebar>
  );
}
