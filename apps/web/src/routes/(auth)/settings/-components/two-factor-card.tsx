import { ORPCError } from "@orpc/client";
import { useMutation } from "@tanstack/react-query";
import { Effect } from "effect";
import { Copy, ShieldCheck, ShieldOff } from "lucide-react";
import { QRCodeSVG } from "qrcode.react";
import { useReducer, useState } from "react";
import { toast } from "sonner";
import { z } from "zod";

import { useAppForm } from "@/components/form/hooks";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  InputOTP,
  InputOTPGroup,
  InputOTPSlot,
} from "@/components/ui/input-otp";
import { Skeleton } from "@/components/ui/skeleton";
import { authClient } from "@/lib/auth-client";

const TOTP_LENGTH = 6;

type Enrollment = {
  totpURI: string;
  backupCodes: string[];
};

type State = {
  enrollment: Enrollment | null;
  showEnableDialog: boolean;
  showDisableDialog: boolean;
  showRegenerateDialog: boolean;
  regeneratedCodes: string[] | null;
};

type Action =
  | { type: "SET_ENROLLMENT"; payload: Enrollment | null }
  | { type: "SET_SHOW_ENABLE"; payload: boolean }
  | { type: "SET_SHOW_DISABLE"; payload: boolean }
  | { type: "SET_SHOW_REGENERATE"; payload: boolean }
  | { type: "SET_REGENERATED_CODES"; payload: string[] | null };

const initialState: State = {
  enrollment: null,
  showEnableDialog: false,
  showDisableDialog: false,
  showRegenerateDialog: false,
  regeneratedCodes: null,
};

function reducer(state: State, action: Action): State {
  switch (action.type) {
    case "SET_ENROLLMENT":
      return { ...state, enrollment: action.payload };
    case "SET_SHOW_ENABLE":
      return { ...state, showEnableDialog: action.payload };
    case "SET_SHOW_DISABLE":
      return { ...state, showDisableDialog: action.payload };
    case "SET_SHOW_REGENERATE":
      return { ...state, showRegenerateDialog: action.payload };
    case "SET_REGENERATED_CODES":
      return { ...state, regeneratedCodes: action.payload };
    default:
      return state;
  }
}

const passwordSchema = z.object({
  password: z.string().min(1, "Obrigatório"),
});

function parseSecretFromUri(totpURI: string): string | null {
  return Effect.runSync(
    Effect.try({
      try: () => new URL(totpURI).searchParams.get("secret"),
      catch: () => new Error("invalid TOTP URI"),
    }).pipe(Effect.catchAll(() => Effect.succeed(null))),
  );
}

export function TwoFactorCard() {
  const { data: session, isPending, refetch } = authClient.useSession();
  const [state, dispatch] = useReducer(reducer, initialState);
  const {
    enrollment,
    showEnableDialog,
    showDisableDialog,
    showRegenerateDialog,
    regeneratedCodes,
  } = state;

  if (isPending || !session) {
    return (
      <Card>
        <CardHeader>
          <CardTitle>Autenticação de dois fatores</CardTitle>
          <CardDescription>
            Adicione uma etapa extra ao login usando um aplicativo autenticador.
          </CardDescription>
        </CardHeader>
        <CardContent>
          <Skeleton className="h-10 w-40" />
        </CardContent>
      </Card>
    );
  }

  const twoFactorEnabled = session.user.twoFactorEnabled === true;

  if (enrollment) {
    return (
      <EnrollmentCard
        enrollment={enrollment}
        onComplete={async () => {
          dispatch({ type: "SET_ENROLLMENT", payload: null });
          await refetch();
        }}
        onCancel={() => dispatch({ type: "SET_ENROLLMENT", payload: null })}
      />
    );
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          {twoFactorEnabled ? (
            <ShieldCheck className="size-5 text-emerald-600" />
          ) : (
            <ShieldOff className="size-5 text-muted-foreground" />
          )}
          Two-factor authentication
        </CardTitle>
        <CardDescription>
          {twoFactorEnabled
            ? "A 2FA está ativada. Você precisará de um código do seu aplicativo autenticador a cada login."
            : "Adicione uma etapa extra ao login usando um aplicativo autenticador (Google Authenticator, 1Password, Authy)."}
        </CardDescription>
      </CardHeader>
      <CardFooter className="flex flex-wrap gap-2">
        {twoFactorEnabled ? (
          <>
            <Button
              variant="outline"
              size="sm"
              onClick={() =>
                dispatch({ type: "SET_SHOW_REGENERATE", payload: true })
              }
            >
              Regenerar códigos de backup
            </Button>
            <Button
              variant="destructive"
              size="sm"
              onClick={() =>
                dispatch({ type: "SET_SHOW_DISABLE", payload: true })
              }
            >
              Desativar 2FA
            </Button>
          </>
        ) : (
          <Button
            size="sm"
            onClick={() => dispatch({ type: "SET_SHOW_ENABLE", payload: true })}
          >
            Ativar 2FA
          </Button>
        )}
      </CardFooter>

      <PasswordDialog
        open={showEnableDialog}
        onOpenChange={(next) =>
          dispatch({ type: "SET_SHOW_ENABLE", payload: next })
        }
        title="Ativar autenticação de dois fatores"
        description="Digite sua senha para iniciar a ativação."
        submitLabel="Continuar"
        onSubmit={async (password) => {
          const result = await authClient.twoFactor.enable({
            password,
            fetchOptions: {
              onError: (e) => {
                throw new ORPCError("ERROR", {
                  message:
                    e.error.message ?? e.error.statusText ?? "Falha ao ativar",
                });
              },
            },
          });
          if (!result.data) return;
          dispatch({
            type: "SET_ENROLLMENT",
            payload: {
              totpURI: result.data.totpURI,
              backupCodes: result.data.backupCodes,
            },
          });
        }}
      />

      <PasswordDialog
        open={showDisableDialog}
        onOpenChange={(next) =>
          dispatch({ type: "SET_SHOW_DISABLE", payload: next })
        }
        title="Desativar autenticação de dois fatores"
        description="Digite sua senha para desativar a 2FA."
        submitLabel="Desativar"
        destructive
        onSubmit={async (password) => {
          await authClient.twoFactor.disable({
            password,
            fetchOptions: {
              onError: (e) => {
                throw new ORPCError("ERROR", {
                  message:
                    e.error.message ??
                    e.error.statusText ??
                    "Falha ao desativar",
                });
              },
            },
          });
          toast.success("Autenticação de dois fatores desativada");
          await refetch();
        }}
      />

      <PasswordDialog
        open={showRegenerateDialog}
        onOpenChange={(next) =>
          dispatch({ type: "SET_SHOW_REGENERATE", payload: next })
        }
        title="Regenerar códigos de backup"
        description="Seus códigos de backup existentes deixarão de funcionar."
        submitLabel="Regenerar"
        onSubmit={async (password) => {
          const result = await authClient.twoFactor.generateBackupCodes({
            password,
            fetchOptions: {
              onError: (e) => {
                throw new ORPCError("ERROR", {
                  message:
                    e.error.message ??
                    e.error.statusText ??
                    "Falha ao regenerar",
                });
              },
            },
          });
          if (!result.data) return;
          dispatch({
            type: "SET_REGENERATED_CODES",
            payload: result.data.backupCodes,
          });
        }}
      />

      <BackupCodesDialog
        codes={regeneratedCodes}
        onClose={() =>
          dispatch({ type: "SET_REGENERATED_CODES", payload: null })
        }
      />
    </Card>
  );
}

function EnrollmentCard({
  enrollment,
  onComplete,
  onCancel,
}: {
  enrollment: Enrollment;
  onComplete: () => Promise<void>;
  onCancel: () => void;
}) {
  const [code, setCode] = useState("");
  const secret = parseSecretFromUri(enrollment.totpURI);

  // oxlint-disable-next-line react-doctor/query-mutation-missing-invalidation
  const verifyMutation = useMutation({
    mutationFn: async (input: string) => {
      return await authClient.twoFactor.verifyTotp({
        code: input,
        fetchOptions: {
          onError: (e) => {
            throw new ORPCError("ERROR", {
              message:
                e.error.message ?? e.error.statusText ?? "Código inválido",
            });
          },
        },
      });
    },
    onSuccess: async () => {
      toast.success("Autenticação de dois fatores ativada");
      await onComplete();
    },
    onError: (error) => {
      toast.error(error.message);
    },
  });

  return (
    <Card>
      <CardHeader>
        <CardTitle>Configurar autenticação de dois fatores</CardTitle>
        <CardDescription>
          Escaneie o QR code com seu aplicativo autenticador e depois digite o
          código de 6 dígitos abaixo para concluir.
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-6">
        <div className="flex flex-col items-center gap-3">
          <div className="rounded-lg border bg-white p-3">
            <QRCodeSVG value={enrollment.totpURI} size={192} />
          </div>
          {secret && (
            <div className="text-center">
              <p className="text-xs text-muted-foreground">
                Ou digite esta chave manualmente:
              </p>
              <code className="text-xs font-mono break-all">{secret}</code>
            </div>
          )}
        </div>

        <div className="space-y-2">
          <h3 className="text-sm font-medium">Códigos de backup</h3>
          <p className="text-xs text-muted-foreground">
            Guarde esses códigos em um local seguro. Cada um pode ser usado uma
            vez para entrar caso você perca acesso ao seu aplicativo
            autenticador.
          </p>
          <div className="grid grid-cols-2 gap-2 rounded-lg border bg-muted/40 p-3 font-mono text-sm">
            {enrollment.backupCodes.map((bc) => (
              <span key={bc}>{bc}</span>
            ))}
          </div>
          <Button
            type="button"
            variant="outline"
            size="sm"
            onClick={async () => {
              await navigator.clipboard.writeText(
                enrollment.backupCodes.join("\n"),
              );
              toast.success("Códigos de backup copiados");
            }}
          >
            <Copy className="mr-1" /> Copiar todos
          </Button>
        </div>

        <form
          action={() => {
            if (code.length === TOTP_LENGTH) {
              verifyMutation.mutate(code);
            }
          }}
          className="space-y-3"
        >
          <h3 className="text-sm font-medium">Verificar com um código</h3>
          <div className="flex justify-center">
            <InputOTP maxLength={TOTP_LENGTH} value={code} onChange={setCode}>
              <InputOTPGroup>
                {Array.from({ length: TOTP_LENGTH }, (_, i) => (
                  <InputOTPSlot key={i} index={i} />
                ))}
              </InputOTPGroup>
            </InputOTP>
          </div>
          <div className="flex gap-2">
            <Button
              type="button"
              variant="outline"
              className="flex-1"
              onClick={onCancel}
            >
              Cancelar
            </Button>
            <Button
              type="submit"
              className="flex-1"
              disabled={code.length !== TOTP_LENGTH || verifyMutation.isPending}
            >
              {verifyMutation.isPending
                ? "Verificando…"
                : "Concluir configuração"}
            </Button>
          </div>
        </form>
      </CardContent>
    </Card>
  );
}

function PasswordDialog({
  open,
  onOpenChange,
  title,
  description,
  submitLabel,
  destructive,
  onSubmit,
}: {
  open: boolean;
  onOpenChange: (next: boolean) => void;
  title: string;
  description: string;
  submitLabel: string;
  destructive?: boolean;
  onSubmit: (password: string) => Promise<void>;
}) {
  const form = useAppForm({
    defaultValues: { password: "" },
    validators: { onChange: passwordSchema },
    onSubmit: async ({ value, formApi }) => {
      await Effect.runPromise(
        Effect.tryPromise({
          try: () => onSubmit(value.password),
          catch: (e): Error => (e instanceof Error ? e : new Error("Falhou")),
        }).pipe(
          Effect.tap(() =>
            Effect.sync(() => {
              formApi.reset();
              onOpenChange(false);
            }),
          ),
          Effect.catchAll((error) =>
            Effect.sync(() => toast.error(error.message)),
          ),
        ),
      );
    },
  });

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
          <DialogDescription>{description}</DialogDescription>
        </DialogHeader>
        <form
          action={async () => {
            await form.handleSubmit();
          }}
          className="space-y-4"
        >
          <form.AppForm>
            <form.AppField name="password">
              {(field) => (
                <div className="space-y-2">
                  <field.Label>Senha atual</field.Label>
                  <field.Input
                    type="password"
                    autoComplete="current-password"
                  />
                  <field.Error />
                </div>
              )}
            </form.AppField>
            <DialogFooter>
              <DialogClose
                render={(props) => (
                  <Button type="button" variant="outline" {...props}>
                    Cancelar
                  </Button>
                )}
              />
              <form.Button
                variant={destructive ? "destructive" : "default"}
                disabled={form.state.isSubmitting}
                loading={form.state.isSubmitting}
                loadingText="Enviando..."
              >
                {submitLabel}
              </form.Button>
            </DialogFooter>
          </form.AppForm>
        </form>
      </DialogContent>
    </Dialog>
  );
}

function BackupCodesDialog({
  codes,
  onClose,
}: {
  codes: string[] | null;
  onClose: () => void;
}) {
  return (
    <Dialog open={codes !== null} onOpenChange={(open) => !open && onClose()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Novos códigos de backup</DialogTitle>
          <DialogDescription>
            Guarde-os em um local seguro. Seus códigos anteriores não funcionam
            mais.
          </DialogDescription>
        </DialogHeader>
        {codes && (
          <div className="grid grid-cols-2 gap-2 rounded-lg border bg-muted/40 p-3 font-mono text-sm">
            {codes.map((bc) => (
              <span key={bc}>{bc}</span>
            ))}
          </div>
        )}
        <DialogFooter>
          <Button
            type="button"
            variant="outline"
            onClick={async () => {
              if (codes) {
                await navigator.clipboard.writeText(codes.join("\n"));
                toast.success("Códigos de backup copiados");
              }
            }}
          >
            <Copy className="mr-1" /> Copiar todos
          </Button>
          <Button type="button" onClick={onClose}>
            Fechar
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
