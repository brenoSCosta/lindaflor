import type { Organization } from "@lindaflor/db/schema/auth";
import { useQuery } from "@tanstack/react-query";
import { ChevronsUpDown, GalleryVerticalEnd } from "lucide-react";
import { toast } from "sonner";

import { Avatar, AvatarFallback, AvatarImage } from "@/components/ui/avatar";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import {
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  useSidebar,
} from "@/components/ui/sidebar";
import { authClient } from "@/lib/auth-client";
import { orpc } from "@/lib/orpc";

const TWENTY_THREE_HOURS_IN_MS = 23 * 60 * 60 * 1000;

function isExternalUrl(value: string): boolean {
  return value.includes("://");
}

export function OrgLogo({
  orgId,
  logo,
  name,
  size = "default",
  className,
}: {
  orgId: string;
  logo: string | null | undefined;
  name: string;
  size?: "default" | "sm" | "lg";
  className?: string;
}) {
  const initial = name?.[0]?.toUpperCase() ?? "?";
  const needsSignedUrl = Boolean(logo && !isExternalUrl(logo));

  const baseKey = orpc.organization.v1.logo.get.queryKey({
    input: { id: orgId },
  });
  const { data } = useQuery({
    ...orpc.organization.v1.logo.get.queryOptions({
      input: { id: orgId },
      staleTime: TWENTY_THREE_HOURS_IN_MS,
      refetchInterval: TWENTY_THREE_HOURS_IN_MS,
      enabled: needsSignedUrl,
    }),
    queryKey: [...baseKey, { logo }],
  });

  const src = (() => {
    if (!logo) {
      return null;
    }
    if (isExternalUrl(logo)) {
      return logo;
    }
    return data?.url ?? null;
  })();

  return (
    <Avatar size={size} className={className}>
      {src ? <AvatarImage src={src} alt={name} /> : undefined}
      <AvatarFallback>{initial}</AvatarFallback>
    </Avatar>
  );
}

const activateOrganization = async ({ id }: { id: Organization["id"] }) => {
  await authClient.organization.setActive(
    { organizationId: id },
    {
      onSuccess: async () => {
        toast.success("Organização ativada com sucesso!");
      },
      onError: (e) => {
        toast.error(e.error.message ?? e.error.statusText);
      },
    },
  );
};

export function OrgSwitcher() {
  const { isMobile } = useSidebar();
  const { data: session } = authClient.useSession();
  const { data: activeOrganization, isPending: isActiveOrgPending } =
    authClient.useActiveOrganization();
  const { data: organizations } = authClient.useListOrganizations();

  if (!session || !organizations || organizations.length === 0) {
    return (
      <SidebarMenu>
        <SidebarMenuItem>
          <SidebarMenuButton size="lg" disabled>
            <div className="flex aspect-square size-8 items-center justify-center rounded-lg bg-sidebar-primary text-sidebar-primary-foreground">
              <GalleryVerticalEnd className="size-4" />
            </div>
            <div className="grid flex-1 text-left text-sm leading-tight">
              <span className="truncate font-medium">OG Service</span>
              <span className="truncate text-xs text-muted-foreground">
                Sem organização
              </span>
            </div>
          </SidebarMenuButton>
        </SidebarMenuItem>
      </SidebarMenu>
    );
  }

  return (
    <SidebarMenu>
      <SidebarMenuItem>
        <DropdownMenu>
          <DropdownMenuTrigger
            render={
              <SidebarMenuButton
                size="lg"
                className="data-[state=open]:bg-sidebar-accent data-[state=open]:text-sidebar-accent-foreground"
                disabled={isActiveOrgPending}
              >
                {activeOrganization ? (
                  <OrgLogo
                    orgId={activeOrganization.id}
                    logo={activeOrganization.logo}
                    name={activeOrganization.name}
                  />
                ) : (
                  <div className="flex aspect-square size-8 items-center justify-center rounded-lg bg-sidebar-primary text-sidebar-primary-foreground">
                    <GalleryVerticalEnd className="size-4" />
                  </div>
                )}
                <div className="grid flex-1 text-left text-sm leading-tight">
                  <span className="truncate font-medium">
                    {activeOrganization?.name ?? "Selecionar organização"}
                  </span>
                  <span className="truncate text-xs text-muted-foreground">
                    Organização
                  </span>
                </div>
                <ChevronsUpDown className="ml-auto" />
              </SidebarMenuButton>
            }
          />
          <DropdownMenuContent
            className="min-w-56 rounded-lg"
            align="start"
            side={isMobile ? "bottom" : "right"}
            sideOffset={4}
          >
            <DropdownMenuGroup>
              <DropdownMenuLabel className="text-xs text-muted-foreground">
                Organizações
              </DropdownMenuLabel>
              {organizations.map((org) => (
                <DropdownMenuItem
                  key={org.id}
                  className="gap-2 p-2"
                  onClick={async () => {
                    await activateOrganization({ id: org.id });
                  }}
                >
                  <OrgLogo
                    orgId={org.id}
                    logo={org.logo}
                    name={org.name}
                    size="sm"
                  />
                  {org.name}
                </DropdownMenuItem>
              ))}
            </DropdownMenuGroup>
          </DropdownMenuContent>
        </DropdownMenu>
      </SidebarMenuItem>
    </SidebarMenu>
  );
}
