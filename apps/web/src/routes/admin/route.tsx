import { defineAbilityFor } from "@lindaflor/shared/lib/ability";
import { parseRoles, toOrgRole } from "@lindaflor/shared/lib/roles";
import {
  createFileRoute,
  Link,
  Outlet,
  redirect,
} from "@tanstack/react-router";

import { Button } from "@/components/ui/button";

export const Route = createFileRoute("/admin")({
  beforeLoad: async ({ context }) => {
    const result = await context.auth.getSession();
    if (result.error || !result.data) {
      throw redirect({ to: "/login" });
    }

    const memberResult = await context.auth.organization.getActiveMember();
    const ability = defineAbilityFor({
      userId: result.data.user.id,
      roles: parseRoles(result.data.user.role),
      activeOrganizationId: result.data.session.activeOrganizationId ?? null,
      orgRole: toOrgRole(memberResult.data?.role),
    });

    if (!ability.can("read", "Product")) {
      throw redirect({ to: "/" });
    }

    return { session: result.data, ability };
  },
  component: AdminLayout,
});

function AdminLayout() {
  return (
    <div className="min-h-screen bg-stone-50">
      <header className="border-b bg-white">
        <div className="mx-auto flex max-w-6xl items-center justify-between p-4">
          <div>
            <p className="text-sm text-stone-500">Painel</p>
            <h1 className="text-xl font-semibold">Linda Flor Admin</h1>
          </div>
          <div className="flex gap-2">
            <Link to="/admin">
              <Button variant="ghost" size="sm">
                Início
              </Button>
            </Link>
            <Link to="/admin/produtos">
              <Button variant="ghost" size="sm">
                Produtos
              </Button>
            </Link>
            <Link to="/admin/pedidos">
              <Button variant="ghost" size="sm">
                Pedidos
              </Button>
            </Link>
            <Link to="/admin/estoque">
              <Button variant="ghost" size="sm">
                Estoque
              </Button>
            </Link>
            <Link to="/">
              <Button variant="outline" size="sm">
                Loja
              </Button>
            </Link>
          </div>
        </div>
      </header>
      <main className="mx-auto max-w-6xl px-4 py-8">
        <Outlet />
      </main>
    </div>
  );
}
