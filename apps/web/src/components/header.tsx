import { env } from "@lindaflor/env/web";
import { useRouter } from "@tanstack/react-router";
import { AlertTriangle, CalendarClock, Check, Globe } from "lucide-react";
import { useCallback } from "react";
import { toast } from "sonner";

import { ThemeToggle } from "@/components/theme-toggle";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { SidebarTrigger } from "@/components/ui/sidebar";
import { useTimeFormat } from "@/context/time-format";
import {
  TIME_FORMAT_GROUP_LABELS,
  TIME_FORMAT_OPTION_GROUPS,
} from "@/context/time-format-options";
import { useTimezone } from "@/context/timezone";
import { TIMEZONE_OPTIONS } from "@/context/timezone-options";
import { authClient } from "@/lib/auth-client";

export function Header() {
  const router = useRouter();
  const { data: session, refetch } = authClient.useSession();
  const { refetch: refetchListOrgs } = authClient.useListOrganizations();
  const { refetch: refetchActiveOrg } = authClient.useActiveOrganization();

  const isImpersonating = Boolean(
    session?.session &&
    "impersonatedBy" in session.session &&
    session.session.impersonatedBy,
  );
  const impersonatedName = session?.user?.name;

  const handleStopImpersonation = async () => {
    await authClient.admin.stopImpersonating({
      fetchOptions: {
        onSuccess: async () => {
          await Promise.all([
            router.invalidate(),
            refetchListOrgs(),
            refetchActiveOrg(),
            refetch(),
          ]);
        },
      },
    });
  };

  return (
    <header className="after:absolute after:bottom-0 after:left-0 after:h-0.5 after:w-full after:bg-border sticky top-0 z-50 w-full bg-background/95 backdrop-blur supports-backdrop-filter:bg-background/60 shadow-sm">
      {isImpersonating && (
        <div className="bg-amber-500/15 text-amber-700 dark:text-amber-300 flex flex-col items-center justify-center gap-2 px-4 py-2 text-sm font-medium sm:flex-row">
          <AlertTriangle className="size-4" />
          <span className="flex gap-2">
            Você está atuando como
            {impersonatedName ? (
              <Badge variant="outline" className="font-semibold">
                {impersonatedName}
              </Badge>
            ) : (
              " outro usuário"
            )}
          </span>
          <Button
            variant="outline"
            size="sm"
            className="ml-2 border-amber-500/30 hover:bg-amber-500/20"
            onClick={() => handleStopImpersonation()}
          >
            Parar de atuar
          </Button>
        </div>
      )}
      <div className="flex min-h-(--header-height) flex-wrap items-center justify-between gap-4 px-4">
        <div className="flex items-center gap-2">
          {session && <SidebarTrigger />}
        </div>
        <div className="flex items-center gap-2">
          {env.VITE_NODE_ENV === "development" && <TimezoneSwitcher />}
          {env.VITE_NODE_ENV === "development" && <TimeFormatSwitcher />}
          <ThemeToggle />
        </div>
      </div>
    </header>
  );
}

function TimezoneSwitcher() {
  const { selectedValue, setTimezone } = useTimezone();

  const handleSelect = useCallback(
    (ianaValue: string) => {
      setTimezone(ianaValue);
      toast.success("Fuso horário alterado com sucesso!");
    },
    [setTimezone],
  );

  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        render={(props) => (
          <Button variant="outline" size="icon" {...props}>
            <Globe className="size-[1.2rem]" />
            <span className="sr-only">Alterar fuso horário</span>
          </Button>
        )}
      />
      <DropdownMenuContent align="end">
        {TIMEZONE_OPTIONS.map((option) => (
          <DropdownMenuItem
            key={option.value}
            onClick={() => handleSelect(option.value)}
          >
            {selectedValue === option.value ? (
              <Check className="mr-2 size-4" />
            ) : (
              <span className="mr-2 w-4" />
            )}
            {option.label}
          </DropdownMenuItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

function TimeFormatSwitcher() {
  const { formatStr, setFormatStr } = useTimeFormat();

  const handleSelect = useCallback(
    (
      value: (typeof TIME_FORMAT_OPTION_GROUPS)[number]["options"][number]["value"],
    ) => {
      setFormatStr(value);
      toast.success("Formato de data alterado com sucesso!");
    },
    [setFormatStr],
  );

  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        render={(props) => (
          <Button variant="outline" size="icon" {...props}>
            <CalendarClock className="size-[1.2rem]" />
            <span className="sr-only">Alterar formato de data</span>
          </Button>
        )}
      />
      <DropdownMenuContent align="end" className="w-64">
        <DropdownMenuRadioGroup
          value={formatStr}
          onValueChange={(value) => handleSelect(value)}
        >
          {TIME_FORMAT_OPTION_GROUPS.map((group, index) => (
            <DropdownMenuGroup key={group.group}>
              {index > 0 && <DropdownMenuSeparator />}
              <DropdownMenuLabel>
                {TIME_FORMAT_GROUP_LABELS[group.group]}
              </DropdownMenuLabel>
              {group.options.map((option) => (
                <DropdownMenuRadioItem key={option.value} value={option.value}>
                  <span className="font-mono text-xs">{option.label}</span>
                </DropdownMenuRadioItem>
              ))}
            </DropdownMenuGroup>
          ))}
        </DropdownMenuRadioGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
