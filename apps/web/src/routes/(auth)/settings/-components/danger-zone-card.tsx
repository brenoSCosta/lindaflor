import React from "react";
import { toast } from "sonner";
import { z } from "zod";

import { useAppForm } from "@/components/form/hooks";
import {
  AlertDialog,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger,
} from "@/components/ui/alert-dialog";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { authClient } from "@/lib/auth-client";

export function DangerZoneCard() {
  const { data: session, isPending } = authClient.useSession();

  if (isPending || !session) {
    return (
      <Card className="border-destructive/40">
        <CardHeader>
          <CardTitle>Zona de perigo</CardTitle>
          <CardDescription>Ações irreversíveis na conta.</CardDescription>
        </CardHeader>
        <CardContent>
          <Skeleton className="h-10 w-40" />
        </CardContent>
      </Card>
    );
  }

  return (
    <Card className="border-destructive/40">
      <CardHeader>
        <CardTitle>Zona de perigo</CardTitle>
        <CardDescription>
          Exclua permanentemente sua conta e todos os dados associados. Isso não
          pode ser desfeito.
        </CardDescription>
      </CardHeader>
      <CardContent>
        <DeleteAccountDialog email={session.user.email} />
      </CardContent>
    </Card>
  );
}

function DeleteAccountDialog({ email }: { email: string }) {
  const [open, setOpen] = React.useState(false);
  const schema = z.object({
    confirmEmail: z
      .string()
      .refine((v) => v === email, { message: "O e-mail não coincide" }),
  });

  const form = useAppForm({
    defaultValues: { confirmEmail: "" },
    validators: { onChange: schema },
    onSubmit: async ({ value: _value, formApi }) => {
      await authClient.deleteUser(
        { callbackURL: "/" },
        {
          onSuccess: () => {
            toast.success(
              "Verifique sua caixa de entrada para confirmar a exclusão",
            );
            formApi.reset();
            setOpen(false);
          },
          onError: (e) => {
            toast.error(e.error.message ?? e.error.statusText);
          },
        },
      );
    },
  });

  return (
    <AlertDialog open={open} onOpenChange={setOpen}>
      <AlertDialogTrigger
        render={(props) => (
          <Button variant="destructive" {...props}>
            Excluir conta
          </Button>
        )}
      />
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>Excluir sua conta?</AlertDialogTitle>
          <AlertDialogDescription>
            Enviaremos um link de confirmação para{" "}
            <span className="font-medium">{email}</span>. Clicar nele excluirá
            permanentemente sua conta e todos os dados associados. Para
            continuar, digite seu e-mail abaixo.
          </AlertDialogDescription>
        </AlertDialogHeader>

        <form
          action={async () => {
            await form.handleSubmit();
          }}
          className="space-y-4"
        >
          <form.AppForm>
            <form.AppField name="confirmEmail">
              {(field) => (
                <field.Field>
                  <field.Label>Seu e-mail</field.Label>
                  <field.Input type="email" autoComplete="off" />
                  <field.Error />
                </field.Field>
              )}
            </form.AppField>

            <AlertDialogFooter>
              <AlertDialogCancel>Cancelar</AlertDialogCancel>
              <form.Button
                variant="destructive"
                disabled={form.state.isSubmitting}
                loading={form.state.isSubmitting}
                loadingText="Enviando..."
              >
                Enviar link de exclusão
              </form.Button>
            </AlertDialogFooter>
          </form.AppForm>
        </form>
      </AlertDialogContent>
    </AlertDialog>
  );
}
