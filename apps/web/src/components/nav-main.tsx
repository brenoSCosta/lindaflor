import { Link, useRouterState, type LinkProps } from "@tanstack/react-router";
import { ChevronRight } from "lucide-react";
import type { ElementType } from "react";

import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import {
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarMenuSub,
  SidebarMenuSubButton,
  SidebarMenuSubItem,
} from "@/components/ui/sidebar";
import type { routeTree } from "@/routeTree.gen";

export type AppNavTo = NonNullable<LinkProps<typeof routeTree>["to"]>;

export type NavMainSubItem = {
  title: string;
  to: AppNavTo;
  icon?: ElementType;
  params?: Record<string, string>;
};

export type NavMainItem = {
  title: string;
  icon: ElementType;
  to?: AppNavTo;
  items?: NavMainSubItem[];
};

export type NavFlatLink = {
  title: string;
  to: AppNavTo;
  icon: ElementType;
};

function isPathActive(pathname: string, to: string): boolean {
  return pathname === to || pathname === `${to}/`;
}

function isItemActive(pathname: string, item: NavMainItem): boolean {
  if (item.to !== undefined && isPathActive(pathname, item.to)) {
    return true;
  }
  return (
    item.items?.some((subItem) => isPathActive(pathname, subItem.to)) ?? false
  );
}

export function NavFlatGroup({
  label,
  links,
}: {
  label: string;
  links: NavFlatLink[];
}) {
  const pathname = useRouterState({
    select: (state) => state.location.pathname,
  });

  if (links.length === 0) {
    return null;
  }

  return (
    <SidebarGroup>
      <SidebarGroupLabel>{label}</SidebarGroupLabel>
      <SidebarGroupContent>
        <SidebarMenu>
          {links.map((link) => (
            <SidebarMenuItem key={link.to}>
              <SidebarMenuButton
                tooltip={link.title}
                isActive={isPathActive(pathname, link.to)}
                render={({ className }) => (
                  <Link to={link.to} className={className}>
                    <link.icon />
                    <span>{link.title}</span>
                  </Link>
                )}
              />
            </SidebarMenuItem>
          ))}
        </SidebarMenu>
      </SidebarGroupContent>
    </SidebarGroup>
  );
}

export function NavMain({
  label,
  items,
}: {
  label?: string;
  items: NavMainItem[];
}) {
  const pathname = useRouterState({
    select: (state) => state.location.pathname,
  });

  if (items.length === 0) {
    return null;
  }

  return (
    <SidebarGroup>
      <SidebarGroupLabel>{label}</SidebarGroupLabel>
      <SidebarMenu>
        {items.map((item) => {
          const hasChildren = (item.items?.length ?? 0) > 0;

          if (!hasChildren && item.to !== undefined) {
            const to = item.to;
            return (
              <SidebarMenuItem key={item.title}>
                <SidebarMenuButton
                  tooltip={item.title}
                  isActive={isPathActive(pathname, to)}
                  render={({ className }) => (
                    <Link to={to} className={className}>
                      <item.icon />
                      <span>{item.title}</span>
                    </Link>
                  )}
                />
              </SidebarMenuItem>
            );
          }

          return (
            <Collapsible
              key={item.title}
              defaultOpen={isItemActive(pathname, item)}
              className="group/collapsible"
            >
              <SidebarMenuItem>
                <CollapsibleTrigger
                  render={<SidebarMenuButton tooltip={item.title} />}
                >
                  <item.icon />
                  <span>{item.title}</span>
                  <ChevronRight className="ml-auto transition-transform duration-200 group-data-[state=open]/collapsible:rotate-90" />
                </CollapsibleTrigger>
                <CollapsibleContent>
                  <SidebarMenuSub>
                    {item.items?.map((subItem) => (
                      <SidebarMenuSubItem key={subItem.title}>
                        <SidebarMenuSubButton
                          isActive={isPathActive(pathname, subItem.to)}
                          render={({ className }) => (
                            <Link
                              to={subItem.to}
                              params={subItem.params}
                              className={className}
                            >
                              {subItem.icon ? <subItem.icon /> : null}
                              <span>{subItem.title}</span>
                            </Link>
                          )}
                        />
                      </SidebarMenuSubItem>
                    ))}
                  </SidebarMenuSub>
                </CollapsibleContent>
              </SidebarMenuItem>
            </Collapsible>
          );
        })}
      </SidebarMenu>
    </SidebarGroup>
  );
}
