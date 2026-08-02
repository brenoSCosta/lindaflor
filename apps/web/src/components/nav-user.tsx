import { Link, useNavigate, type LinkProps } from "@tanstack/react-router";
import {
  BadgeCheck,
  ChevronsUpDown,
  LogOut,
  Shield,
  Users,
} from "lucide-react";
import React from "react";
import type { z } from "zod";

import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import {
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  useSidebar,
} from "@/components/ui/sidebar";
import { Skeleton } from "@/components/ui/skeleton";
import { UserAvatar } from "@/components/user-avatar";
import { authClient } from "@/lib/auth-client";
import { tabSchema } from "@/routes/(auth)/settings/index";
import type { routeTree } from "@/routeTree.gen";

type MenuLink =
  | {
      to: "/settings";
      label: string;
      icon: React.ElementType;
      search: { tab: z.infer<typeof tabSchema> };
      visible?: boolean;
    }
  | {
      to: Exclude<LinkProps<typeof routeTree>["to"], "/settings">;
      label: string;
      icon: React.ElementType;
      visible?: boolean;
    };

type MenuAction = {
  label: string;
  icon: React.ElementType;
  variant?: "default" | "destructive";
  onClick: () => Promise<void> | void;
  visible?: boolean;
};

type MenuGroup = {
  label?: string;
  links?: MenuLink[];
  actions?: MenuAction[];
};

export function NavUser() {
  const { isMobile } = useSidebar();
  const navigate = useNavigate();
  const { data: session, isPending } = authClient.useSession();

  if (isPending) {
    return (
      <SidebarMenu>
        <SidebarMenuItem>
          <Skeleton className="h-12 w-full" />
        </SidebarMenuItem>
      </SidebarMenu>
    );
  }

  if (!session) {
    return (
      <SidebarMenu>
        <SidebarMenuItem>
          <SidebarMenuButton
            size="lg"
            render={({ className }) => (
              <Link to="/login" className={className}>
                <div className="flex aspect-square size-8 items-center justify-center rounded-lg bg-sidebar-primary text-sidebar-primary-foreground">
                  <LogOut className="size-4" />
                </div>
                <div className="grid flex-1 text-left text-sm leading-tight">
                  <span className="truncate font-medium">Entrar</span>
                  <span className="truncate text-xs text-muted-foreground">
                    Acessar conta
                  </span>
                </div>
              </Link>
            )}
          />
        </SidebarMenuItem>
      </SidebarMenu>
    );
  }

  const { id, name, email, image } = session.user;

  const groups: MenuGroup[] = [
    {
      label: "Configurações",
      links: [
        {
          to: "/settings",
          label: "Perfil",
          icon: Users,
          search: { tab: "profile" },
        },
        {
          to: "/settings",
          label: "Conta",
          icon: BadgeCheck,
          search: { tab: "account" },
        },
        {
          to: "/settings",
          label: "Segurança",
          icon: Shield,
          search: { tab: "security" },
        },
      ],
    },
    {
      actions: [
        {
          label: "Sair",
          icon: LogOut,
          variant: "destructive",
          onClick: async () => {
            await authClient.signOut({
              fetchOptions: {
                onSuccess: async () => {
                  await navigate({ to: "/" });
                },
              },
            });
          },
        },
      ],
    },
  ];

  return (
    <SidebarMenu>
      <SidebarMenuItem>
        <DropdownMenu>
          <DropdownMenuTrigger
            render={
              <SidebarMenuButton
                size="lg"
                className="data-[state=open]:bg-sidebar-accent data-[state=open]:text-sidebar-accent-foreground"
              >
                <UserAvatar
                  userId={id}
                  image={image}
                  name={name}
                  className="size-8 rounded-lg"
                />
                <div className="grid flex-1 text-left text-sm leading-tight">
                  <span className="truncate font-medium">{name}</span>
                  <span className="truncate text-xs">{email}</span>
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
              <DropdownMenuLabel className="p-0 font-normal">
                <div className="flex items-center gap-2 px-1 py-1.5 text-left text-sm">
                  <UserAvatar
                    userId={id}
                    image={image}
                    name={name}
                    className="size-8"
                  />
                  <div className="grid flex-1 text-left text-sm leading-tight">
                    <span className="truncate font-medium">{name}</span>
                    <span className="truncate text-xs">{email}</span>
                  </div>
                </div>
              </DropdownMenuLabel>
            </DropdownMenuGroup>
            {groups.map((group) => {
              const groupKey = group.label ?? "default";
              const visibleLinks = group.links?.filter(
                (link) => link.visible === undefined || link.visible,
              );
              const visibleActions = group.actions?.filter(
                (action) => action.visible === undefined || action.visible,
              );
              if (
                (!visibleLinks || visibleLinks.length === 0) &&
                (!visibleActions || visibleActions.length === 0)
              ) {
                return null;
              }
              return (
                <React.Fragment key={groupKey}>
                  <DropdownMenuSeparator />
                  <DropdownMenuGroup>
                    {group.label && (
                      <DropdownMenuLabel>{group.label}</DropdownMenuLabel>
                    )}
                    {visibleLinks?.map((link) => {
                      if ("search" in link) {
                        return (
                          <DropdownMenuItem
                            key={link.label}
                            render={({ className }) => (
                              <Link
                                to={link.to}
                                search={link.search}
                                className={className}
                              >
                                <link.icon />
                                {link.label}
                              </Link>
                            )}
                          />
                        );
                      }
                      return (
                        <DropdownMenuItem
                          key={link.label}
                          render={({ className }) => (
                            <Link to={link.to} className={className}>
                              <link.icon />
                              {link.label}
                            </Link>
                          )}
                        />
                      );
                    })}
                    {visibleActions?.map((action) => {
                      const Icon = action.icon;
                      return (
                        <DropdownMenuItem
                          key={action.label}
                          variant={action.variant}
                          onClick={() => void action.onClick?.()}
                        >
                          <Icon />
                          {action.label}
                        </DropdownMenuItem>
                      );
                    })}
                  </DropdownMenuGroup>
                </React.Fragment>
              );
            })}
          </DropdownMenuContent>
        </DropdownMenu>
      </SidebarMenuItem>
    </SidebarMenu>
  );
}
