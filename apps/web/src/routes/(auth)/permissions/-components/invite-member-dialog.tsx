import type { OrgRoles } from "@lindaflor/shared/lib/roles";
import { UserPlus } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";
import { z } from "zod";

import { useAppForm } from "@/components/form/hooks";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { authClient } from "@/lib/auth-client";

const ROLE_OPTIONS: ReadonlyArray<{ value: OrgRoles; label: string }> = [
  { value: "member", label: "Membro" },
  { value: "admin", label: "Administrador" },
  { value: "owner", label: "Proprietário" },
];

const schema = z.object({
  email: z.email("Endereço de e-mail inválido"),
  role: z.enum(["member", "admin", "owner"]),
});

export function InviteMemberDialog({
  organizationId,
  onInvited,
}: {
  organizationId: string;
  onInvited: () => Promise<void> | void;
}) {
  const [open, setOpen] = useState(false);

  const defaultValues: { email: string; role: OrgRoles } = {
    email: "",
    role: "member",
  };

  const form = useAppForm({
    defaultValues,
    validators: { onChange: schema },
    onSubmit: async ({ value }) => {
      await authClient.organization.inviteMember(
        {
          email: value.email,
          role: value.role,
          organizationId,
        },
        {
          onSuccess: async () => {
            toast.success(`Convite enviado para ${value.email}`);
            form.reset();
            setOpen(false);
            await onInvited();
          },
          onError: (e) => {
            toast.error(e.error.message ?? "Falha ao enviar o convite");
          },
        },
      );
    },
  });

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger render={<Button size="sm" />}>
        <UserPlus className="size-4" />
        Convidar membro
      </DialogTrigger>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Convidar um membro</DialogTitle>
          <DialogDescription>
            Envie um e-mail de convite para adicionar alguém a esta organização.
          </DialogDescription>
        </DialogHeader>
        <form
          action={async () => {
            await form.handleSubmit();
          }}
        >
          <form.AppForm>
            <div className="grid gap-4">
              <form.AppField name="email">
                {(field) => (
                  <field.Field>
                    <field.Label>E-mail</field.Label>
                    <field.Input
                      type="email"
                      placeholder="colega@exemplo.com"
                    />
                    <field.Error />
                  </field.Field>
                )}
              </form.AppField>
              <form.AppField name="role">
                {(field) => (
                  <field.Field>
                    <field.Label>Papel</field.Label>
                    <field.Select>
                      <field.SelectTrigger />
                      <field.SelectContent>
                        {ROLE_OPTIONS.map((option) => (
                          <field.SelectItem
                            key={option.value}
                            value={option.value}
                          >
                            {option.label}
                          </field.SelectItem>
                        ))}
                      </field.SelectContent>
                    </field.Select>
                    <field.Error />
                  </field.Field>
                )}
              </form.AppField>
            </div>
            <DialogFooter className="mt-6">
              <Button
                type="button"
                variant="outline"
                onClick={() => setOpen(false)}
              >
                Cancelar
              </Button>
              <form.Button
                disabled={form.state.isSubmitting}
                loading={form.state.isSubmitting}
                loadingText="Enviando..."
              >
                Enviar convite
              </form.Button>
            </DialogFooter>
          </form.AppForm>
        </form>
      </DialogContent>
    </Dialog>
  );
}
