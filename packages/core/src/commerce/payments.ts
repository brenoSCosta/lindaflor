import { createHmac, timingSafeEqual } from "node:crypto";

import { confirmFulfillmentSale } from "@lindaflor/core/commerce/inventory";
import {
  buildWhatsAppUrl,
  generateStaticPixPayload,
  renderWhatsAppTemplate,
} from "@lindaflor/core/commerce/pix-static";
import {
  getStoreSettings,
  isStaticPixConfigured,
} from "@lindaflor/core/commerce/store-settings";
import type { Database } from "@lindaflor/db";
import { db } from "@lindaflor/db";
import { env } from "@lindaflor/env/server";
import type { StoreOrder } from "@lindaflor/shared/schemas/commerce";
import { ORPCError } from "@orpc/server";
import QRCode from "qrcode";
import { z } from "zod";

type PaymentMeta = NonNullable<StoreOrder["payment_meta"]>;

type DbTx = Parameters<Parameters<Database["transaction"]>[0]>[0];

const mercadoPagoCreatePaymentSchema = z.object({
  id: z.number(),
  point_of_interaction: z
    .object({
      transaction_data: z
        .object({
          qr_code: z.string().optional(),
          qr_code_base64: z.string().optional(),
          ticket_url: z.string().optional(),
        })
        .optional(),
    })
    .optional(),
});

const mercadoPagoPaymentSchema = z.object({
  id: z.number(),
  status: z.string(),
  external_reference: z.string().optional(),
});

function parseSignaturePart(
  part: string,
): { key: string; value: string } | null {
  const separatorIndex = part.indexOf("=");
  if (separatorIndex === -1) {
    return null;
  }

  return {
    key: part.slice(0, separatorIndex),
    value: part.slice(separatorIndex + 1),
  };
}

const brlFormatter = new Intl.NumberFormat("pt-BR", {
  style: "currency",
  currency: "BRL",
});

function formatBrl(cents: number) {
  return brlFormatter.format(cents / 100);
}

async function createStaticPixPayment(params: {
  orderId: string;
  total_cents: number;
}): Promise<PaymentMeta> {
  const settings = await getStoreSettings();
  if (!isStaticPixConfigured(settings) || !settings.pix_key_type) {
    throw new ORPCError("SERVICE_UNAVAILABLE", {
      message: "Pagamento PIX não configurado",
    });
  }

  const pix_copy_paste = generateStaticPixPayload({
    pix_key: settings.pix_key ?? "",
    pix_key_type: settings.pix_key_type,
    merchant_name: settings.pix_merchant_name ?? "",
    merchant_city: settings.pix_merchant_city ?? "",
    amount_cents: params.total_cents,
    order_id: params.orderId,
  });

  const pix_qr_base64 = (
    await QRCode.toDataURL(pix_copy_paste, {
      errorCorrectionLevel: "M",
      margin: 1,
      width: 256,
    })
  ).replace(/^data:image\/png;base64,/, "");

  const message = renderWhatsAppTemplate(settings.whatsapp_message_template, {
    order_id: params.orderId,
    total: formatBrl(params.total_cents),
  });

  const ticket_url = settings.whatsapp_number
    ? buildWhatsAppUrl({
        phone: settings.whatsapp_number,
        message,
      })
    : undefined;

  return {
    provider: "static_pix",
    pix_copy_paste,
    pix_qr_base64,
    ticket_url: ticket_url ?? undefined,
  };
}

async function createMercadoPagoPixPayment(params: {
  orderId: string;
  total_cents: number;
  guest_email: string;
  description: string;
}): Promise<PaymentMeta> {
  if (!env.MERCADO_PAGO_ACCESS_TOKEN) {
    throw new ORPCError("SERVICE_UNAVAILABLE", {
      message: "Mercado Pago não configurado",
    });
  }

  const amount = params.total_cents / 100;

  const response = await fetch("https://api.mercadopago.com/v1/payments", {
    method: "POST",
    headers: {
      Authorization: `Bearer ${env.MERCADO_PAGO_ACCESS_TOKEN}`,
      "Content-Type": "application/json",
      "X-Idempotency-Key": params.orderId,
    },
    body: JSON.stringify({
      transaction_amount: amount,
      description: params.description,
      payment_method_id: "pix",
      payer: { email: params.guest_email },
      external_reference: params.orderId,
    }),
  });

  if (!response.ok) {
    const body = await response.text();
    throw new ORPCError("SERVICE_UNAVAILABLE", {
      message: `Falha ao criar pagamento PIX: ${body}`,
    });
  }

  const parsed = mercadoPagoCreatePaymentSchema.safeParse(
    await response.json(),
  );
  if (!parsed.success) {
    throw new ORPCError("SERVICE_UNAVAILABLE", {
      message: "Resposta inválida do Mercado Pago",
    });
  }

  const data = parsed.data;
  const transaction = data.point_of_interaction?.transaction_data;

  return {
    provider: "mercado_pago",
    external_id: String(data.id),
    pix_copy_paste: transaction?.qr_code,
    pix_qr_base64: transaction?.qr_code_base64,
    ticket_url: transaction?.ticket_url,
  };
}

/**
 * Payment priority:
 * 1. Static PIX from store_settings
 * 2. Mercado Pago when access token is set
 * 3. Otherwise fail — never create fake placeholder PIX in production
 */
export async function createPixPayment(params: {
  orderId: string;
  total_cents: number;
  guest_email: string;
  description: string;
}): Promise<PaymentMeta> {
  const settings = await getStoreSettings();

  if (isStaticPixConfigured(settings)) {
    return createStaticPixPayment({
      orderId: params.orderId,
      total_cents: params.total_cents,
    });
  }

  if (env.MERCADO_PAGO_ACCESS_TOKEN) {
    return createMercadoPagoPixPayment(params);
  }

  throw new ORPCError("SERVICE_UNAVAILABLE", {
    message:
      "Pagamento não configurado. Configure a chave PIX em Configurações da loja ou o Mercado Pago.",
  });
}

export async function confirmOrderPayment(
  orderId: string,
  externalTx?: DbTx,
): Promise<string | null> {
  const run = async (tx: DbTx) => {
    const [{ order_items, orders }, { eq }] = await Promise.all([
      import("@lindaflor/db/schema/commerce"),
      import("drizzle-orm"),
    ]);

    const [order] = await tx
      .select({ id: orders.id, status: orders.status })
      .from(orders)
      .where(eq(orders.id, orderId))
      .limit(1);

    if (!order) {
      return null;
    }

    if (order.status === "paid") {
      return orderId;
    }

    if (order.status !== "pending_payment") {
      return null;
    }

    const items = await tx
      .select()
      .from(order_items)
      .where(eq(order_items.order_id, orderId));

    await Promise.all(
      items.map((item) =>
        confirmFulfillmentSale(
          {
            variant_id: item.variant_id,
            quantity: item.quantity,
            order_id: orderId,
            warehouse_id: item.warehouse_id,
          },
          tx,
        ),
      ),
    );

    await tx
      .update(orders)
      .set({ status: "paid" })
      .where(eq(orders.id, orderId));

    return orderId;
  };

  if (externalTx) {
    return run(externalTx);
  }

  return db.transaction(run);
}

async function fetchMercadoPagoPayment(paymentId: string) {
  if (!env.MERCADO_PAGO_ACCESS_TOKEN) {
    return null;
  }

  const response = await fetch(
    `https://api.mercadopago.com/v1/payments/${paymentId}`,
    {
      headers: {
        Authorization: `Bearer ${env.MERCADO_PAGO_ACCESS_TOKEN}`,
      },
    },
  );

  if (!response.ok) {
    return null;
  }

  const parsed = mercadoPagoPaymentSchema.safeParse(await response.json());
  return parsed.success ? parsed.data : null;
}

export function verifyMercadoPagoWebhookSignature(
  request: Request,
  _bodyText: string,
) {
  const secret = env.MERCADO_PAGO_WEBHOOK_SECRET;
  if (!secret) {
    return true;
  }

  const signature = request.headers.get("x-signature");
  const requestId = request.headers.get("x-request-id");
  if (!signature || !requestId) {
    return false;
  }

  const parts = Object.fromEntries(
    signature
      .split(",")
      .map(parseSignaturePart)
      .filter((part): part is { key: string; value: string } => part !== null)
      .map((part) => [part.key, part.value]),
  );
  const ts = parts.ts;
  const v1 = parts.v1;
  if (!ts || !v1) {
    return false;
  }

  const manifest = `id:${requestId};request-id:${requestId};ts:${ts};`;
  const expected = createHmac("sha256", secret).update(manifest).digest("hex");

  try {
    return timingSafeEqual(Buffer.from(v1), Buffer.from(expected));
  } catch {
    return false;
  }
}

export async function handleMercadoPagoNotification(paymentId: string) {
  const payment = await fetchMercadoPagoPayment(paymentId);
  if (!payment) {
    return { ok: false as const, reason: "payment_not_found" };
  }

  if (payment.status !== "approved") {
    return { ok: true as const, status: payment.status };
  }

  const orderId = payment.external_reference;
  if (!orderId) {
    return { ok: false as const, reason: "missing_external_reference" };
  }

  const confirmed = await confirmOrderPayment(orderId);
  return { ok: true as const, status: "approved", orderId: confirmed };
}

export async function markOrderPaidFromWebhook(externalId: string) {
  const [{ orders }, { sql }] = await Promise.all([
    import("@lindaflor/db/schema/commerce"),
    import("drizzle-orm"),
  ]);

  const [order] = await db
    .select({ id: orders.id })
    .from(orders)
    .where(sql`${orders.payment_meta}->>'external_id' = ${externalId}`)
    .limit(1);

  if (!order) {
    return null;
  }

  return confirmOrderPayment(order.id);
}
