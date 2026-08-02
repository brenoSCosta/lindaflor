import { ORPCError } from "@orpc/client";
import { useMutation } from "@tanstack/react-query";
import { Link, useNavigate } from "@tanstack/react-router";
import { useState } from "react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import {
  InputOTP,
  InputOTPGroup,
  InputOTPSlot,
} from "@/components/ui/input-otp";
import { Label } from "@/components/ui/label";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { authClient } from "@/lib/auth-client";

const TOTP_LENGTH = 6;

function TrustDeviceCheckbox({
  checked,
  onChange,
}: {
  checked: boolean;
  onChange: (next: boolean) => void;
}) {
  return (
    <div className="flex items-center gap-2">
      <Checkbox
        id="trust-device"
        checked={checked}
        onCheckedChange={(value) => onChange(value)}
      />
      <Label htmlFor="trust-device" className="text-sm font-normal">
        Confiar neste dispositivo por 30 dias
      </Label>
    </div>
  );
}

export function TwoFactor() {
  const navigate = useNavigate();
  const [trustDevice, setTrustDevice] = useState(false);
  const [totpCode, setTotpCode] = useState("");
  const [backupCode, setBackupCode] = useState("");

  // oxlint-disable-next-line react-doctor/query-mutation-missing-invalidation
  const totpMutation = useMutation({
    mutationFn: async (code: string) => {
      return await authClient.twoFactor.verifyTotp({
        code,
        trustDevice,
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
      toast.success("Login realizado");
      void navigate({ to: "/dashboard" });
    },
    onError: (error) => {
      toast.error(error.message);
    },
  });

  // oxlint-disable-next-line react-doctor/query-mutation-missing-invalidation
  const backupMutation = useMutation({
    mutationFn: async (code: string) => {
      return await authClient.twoFactor.verifyBackupCode({
        code,
        trustDevice,
        fetchOptions: {
          onError: (e) => {
            throw new ORPCError("ERROR", {
              message:
                e.error.message ??
                e.error.statusText ??
                "Código de backup inválido",
            });
          },
        },
      });
    },
    onSuccess: async () => {
      toast.success("Login realizado");
      void navigate({ to: "/dashboard" });
    },
    onError: (error) => {
      toast.error(error.message);
    },
  });

  return (
    <div className="mx-auto w-full mt-10 max-w-md p-6">
      <h1 className="mb-2 text-center text-3xl font-bold">
        Autenticação de dois fatores
      </h1>
      <p className="mb-6 text-center text-sm text-muted-foreground">
        Digite o código do seu aplicativo autenticador ou use um código de
        backup.
      </p>

      <Tabs defaultValue="authenticator" className="flex flex-col">
        <TabsList className="grid w-full grid-cols-2">
          <TabsTrigger value="authenticator">Autenticador</TabsTrigger>
          <TabsTrigger value="backup">Código de backup</TabsTrigger>
        </TabsList>

        <TabsContent value="authenticator" className="space-y-4 pt-4">
          <form
            action={() => {
              if (totpCode.length === TOTP_LENGTH) {
                totpMutation.mutate(totpCode);
              }
            }}
            className="space-y-4"
          >
            <div className="flex justify-center">
              <InputOTP
                maxLength={TOTP_LENGTH}
                value={totpCode}
                onChange={setTotpCode}
              >
                <InputOTPGroup>
                  {Array.from({ length: TOTP_LENGTH }, (_, i) => (
                    <InputOTPSlot key={i} index={i} />
                  ))}
                </InputOTPGroup>
              </InputOTP>
            </div>

            <TrustDeviceCheckbox
              checked={trustDevice}
              onChange={setTrustDevice}
            />

            <Button
              type="submit"
              className="w-full"
              disabled={
                totpCode.length !== TOTP_LENGTH || totpMutation.isPending
              }
            >
              {totpMutation.isPending ? "Verificando…" : "Verificar"}
            </Button>
          </form>
        </TabsContent>

        <TabsContent value="backup" className="space-y-4 pt-4">
          <form
            action={() => {
              if (backupCode.trim().length > 0) {
                backupMutation.mutate(backupCode.trim());
              }
            }}
            className="space-y-4"
          >
            <div className="space-y-2">
              <Label htmlFor="backup-code">Código de backup</Label>
              <input
                id="backup-code"
                type="text"
                aria-label="Código de backup"
                autoComplete="off"
                value={backupCode}
                onChange={(e) => setBackupCode(e.target.value)}
                className="w-full rounded-lg border border-input bg-background px-3 py-2 text-sm font-mono"
                placeholder="xxxx-xxxx"
              />
            </div>

            <TrustDeviceCheckbox
              checked={trustDevice}
              onChange={setTrustDevice}
            />

            <Button
              type="submit"
              className="w-full"
              disabled={
                backupCode.trim().length === 0 || backupMutation.isPending
              }
            >
              {backupMutation.isPending ? "Verificando…" : "Verificar"}
            </Button>
          </form>
        </TabsContent>
      </Tabs>

      <div className="mt-6 text-center">
        <Link
          to="/login"
          className="text-sm text-indigo-600 hover:text-indigo-800"
        >
          Cancelar e fazer login novamente
        </Link>
      </div>
    </div>
  );
}
