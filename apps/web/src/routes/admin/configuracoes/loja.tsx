import { pixKeyTypes } from "@lindaflor/shared/schemas/commerce";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { createFileRoute, Link } from "@tanstack/react-router";
import { QRCodeSVG } from "qrcode.react";
import { useEffect, useState, type ComponentProps } from "react";
import { toast } from "sonner";
import { z } from "zod";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Textarea } from "@/components/ui/textarea";
import { orpc } from "@/lib/orpc";

export const Route = createFileRoute("/admin/configuracoes/loja")({
  component: AdminStoreSettingsPage,
});

const pixKeyTypeSchema = z.enum(pixKeyTypes);

const pixKeyTypeLabels: Record<(typeof pixKeyTypes)[number], string> = {
  cpf: "CPF",
  cnpj: "CNPJ",
  email: "E-mail",
  phone: "Telefone",
  random: "Chave aleatória",
};

function AdminStoreSettingsPage() {
  const queryClient = useQueryClient();
  const settingsQuery = useQuery(
    orpc.commerce.admin.getStoreSettings.queryOptions({ input: undefined }),
  );

  const [form, setForm] = useState({
    pix_key: "",
    pix_key_type: "" as "" | (typeof pixKeyTypes)[number],
    pix_merchant_name: "",
    pix_merchant_city: "",
    whatsapp_number: "",
    whatsapp_message_template: "",
  });

  useEffect(() => {
    if (!settingsQuery.data) {
      return;
    }
    setForm({
      pix_key: settingsQuery.data.pix_key ?? "",
      pix_key_type: settingsQuery.data.pix_key_type ?? "",
      pix_merchant_name: settingsQuery.data.pix_merchant_name ?? "",
      pix_merchant_city: settingsQuery.data.pix_merchant_city ?? "",
      whatsapp_number: settingsQuery.data.whatsapp_number ?? "",
      whatsapp_message_template:
        settingsQuery.data.whatsapp_message_template ?? "",
    });
  }, [settingsQuery.data]);

  const updateMutation = useMutation(
    orpc.commerce.admin.updateStoreSettings.mutationOptions({
      onSuccess: async () => {
        await queryClient.invalidateQueries({
          queryKey: orpc.commerce.admin.getStoreSettings.key(),
        });
        toast.success("Configurações salvas");
      },
      onError: (error) => toast.error(error.message),
    }),
  );

  const handleSubmit: NonNullable<ComponentProps<"form">["onSubmit"]> = (
    event,
  ) => {
    event.preventDefault();
    const keyType = form.pix_key_type
      ? pixKeyTypeSchema.safeParse(form.pix_key_type)
      : null;

    updateMutation.mutate({
      pix_key: form.pix_key.trim() || null,
      pix_key_type: keyType?.success ? keyType.data : null,
      pix_merchant_name: form.pix_merchant_name.trim() || null,
      pix_merchant_city: form.pix_merchant_city.trim() || null,
      whatsapp_number: form.whatsapp_number.trim() || null,
      whatsapp_message_template: form.whatsapp_message_template.trim() || null,
    });
  };

  if (settingsQuery.isLoading) {
    return <p>Carregando configurações…</p>;
  }

  if (settingsQuery.isError) {
    return (
      <p className="text-red-600">
        Não foi possível carregar as configurações da loja.
      </p>
    );
  }

  const previewReady =
    form.pix_key.trim() &&
    form.pix_key_type &&
    form.pix_merchant_name.trim() &&
    form.pix_merchant_city.trim();

  return (
    <div className="space-y-6">
      <div>
        <p className="text-sm text-stone-500">
          <Link to="/admin">Admin</Link> / Configurações
        </p>
        <h2 className="text-2xl font-semibold">Configurações da loja</h2>
        <p className="text-stone-600">
          Chave PIX estática e WhatsApp usados no checkout. Se a chave PIX
          estiver completa, ela tem prioridade sobre o Mercado Pago.
        </p>
      </div>

      <form
        onSubmit={handleSubmit}
        className="max-w-2xl space-y-6 rounded-xl border bg-white p-6"
      >
        <fieldset className="space-y-4">
          <legend className="font-medium">PIX (chave fixa)</legend>
          <div className="space-y-2">
            <Label htmlFor="pix_key_type">Tipo da chave</Label>
            <select
              id="pix_key_type"
              aria-label="Tipo da chave PIX"
              className="h-10 w-full rounded-md border px-3 text-sm"
              value={form.pix_key_type}
              onChange={(e) => {
                const parsed = pixKeyTypeSchema.safeParse(e.target.value);
                setForm((current) => ({
                  ...current,
                  pix_key_type: parsed.success ? parsed.data : "",
                }));
              }}
            >
              <option value="">Selecione…</option>
              {pixKeyTypes.map((type) => (
                <option key={type} value={type}>
                  {pixKeyTypeLabels[type]}
                </option>
              ))}
            </select>
          </div>
          <div className="space-y-2">
            <Label htmlFor="pix_key">Chave PIX</Label>
            <Input
              id="pix_key"
              value={form.pix_key}
              onChange={(e) =>
                setForm((current) => ({ ...current, pix_key: e.target.value }))
              }
              placeholder="CPF, CNPJ, e-mail, telefone ou chave aleatória"
            />
          </div>
          <div className="grid gap-4 md:grid-cols-2">
            <div className="space-y-2">
              <Label htmlFor="pix_merchant_name">Nome do recebedor</Label>
              <Input
                id="pix_merchant_name"
                maxLength={25}
                value={form.pix_merchant_name}
                onChange={(e) =>
                  setForm((current) => ({
                    ...current,
                    pix_merchant_name: e.target.value,
                  }))
                }
                placeholder="Linda Flor"
              />
              <p className="text-xs text-stone-500">
                Máx. 25 caracteres (Bacen)
              </p>
            </div>
            <div className="space-y-2">
              <Label htmlFor="pix_merchant_city">Cidade</Label>
              <Input
                id="pix_merchant_city"
                maxLength={15}
                value={form.pix_merchant_city}
                onChange={(e) =>
                  setForm((current) => ({
                    ...current,
                    pix_merchant_city: e.target.value,
                  }))
                }
                placeholder="Aracaju"
              />
              <p className="text-xs text-stone-500">
                Máx. 15 caracteres (Bacen)
              </p>
            </div>
          </div>
        </fieldset>

        <fieldset className="space-y-4">
          <legend className="font-medium">WhatsApp da loja</legend>
          <div className="space-y-2">
            <Label htmlFor="whatsapp_number">Número (com DDI)</Label>
            <Input
              id="whatsapp_number"
              value={form.whatsapp_number}
              onChange={(e) =>
                setForm((current) => ({
                  ...current,
                  whatsapp_number: e.target.value,
                }))
              }
              placeholder="5579998165115"
            />
          </div>
          <div className="space-y-2">
            <Label htmlFor="whatsapp_message_template">
              Modelo da mensagem do cliente
            </Label>
            <Textarea
              id="whatsapp_message_template"
              value={form.whatsapp_message_template}
              onChange={(e) =>
                setForm((current) => ({
                  ...current,
                  whatsapp_message_template: e.target.value,
                }))
              }
              rows={3}
            />
            <p className="text-xs text-stone-500">
              Use {"{{order_id}}"} e {"{{total}}"} como placeholders.
            </p>
          </div>
        </fieldset>

        {previewReady ? (
          <div className="rounded-lg border bg-stone-50 p-4">
            <p className="mb-2 text-sm font-medium">Prévia da chave</p>
            <p className="mb-3 break-all text-xs text-stone-600">
              {form.pix_key}
            </p>
            <div className="inline-block bg-white p-2">
              <QRCodeSVG value={form.pix_key} size={128} />
            </div>
            <p className="mt-2 text-xs text-stone-500">
              O QR do pedido usa o payload EMV completo (valor + txid), gerado
              no checkout.
            </p>
          </div>
        ) : null}

        <Button type="submit" disabled={updateMutation.isPending}>
          {updateMutation.isPending ? "Salvando…" : "Salvar configurações"}
        </Button>
      </form>
    </div>
  );
}
