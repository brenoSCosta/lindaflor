import { env } from "@lindaflor/env/server";
import { createHmac, timingSafeEqual } from "node:crypto";
import type { StoreOrder } from "@lindaflor/shared/schemas/commerce";

import { confirmFulfillmentSale } from "@lindaflor/core/commerce/inventory";

type PaymentMeta = NonNullable<StoreOrder["payment_meta"]>;

export async function createPixPayment(params: {
  orderId: string;
  total_cents: number;
  guest_email: string;
  description: string;
}): Promise<PaymentMeta> {
  const amount = params.total_cents / 100;

  if (!env.MERCADO_PAGO_ACCESS_TOKEN) {
    const copy = `00020126580014BR.GOV.BCB.PIX0136${params.orderId}520400005303986540${amount.toFixed(2)}5802BR5925Linda Flor Moda Praia6009ARACAJU62070503***6304ABCD`;

    return {
      provider: "manual",
      pix_copy_paste: copy,
      ticket_url: `https://wa.me/5579998165115?text=${encodeURIComponent(
        `Olá! Fiz o pedido ${params.orderId} e quero pagar via PIX.`,
      )}`,
    };
  }

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
    throw new Error(`Mercado Pago error: ${body}`);
  }

  const data = (await response.json()) as {
    id: number;
    point_of_interaction?: {
      transaction_data?: {
        qr_code?: string;
        qr_code_base64?: string;
        ticket_url?: string;
      };
    };
  };

  const transaction = data.point_of_interaction?.transaction_data;

  return {
    provider: "mercado_pago",
    external_id: String(data.id),
    pix_copy_paste: transaction?.qr_code,
    pix_qr_base64: transaction?.qr_code_base64,
    ticket_url: transaction?.ticket_url,
  };
}

export async function confirmOrderPayment(orderId: string) {
  const { db } = await import("@lindaflor/db");
  const { order_items, orders } = await import("@lindaflor/db/schema/commerce");
  const { eq } = await import("drizzle-orm");

  return db.transaction(async (tx) => {
    const [order] = await tx
      .select({ id: orders.id, status: orders.status })
      .from(orders)
      .where(eq(orders.id, orderId))
      .limit(1);

    if (!order || order.status !== "pending_payment") {
      return null;
    }

    const items = await tx
      .select()
      .from(order_items)
      .where(eq(order_items.order_id, orderId));

    for (const item of items) {
      await confirmFulfillmentSale({
        variant_id: item.variant_id,
        quantity: item.quantity,
        order_id: orderId,
      });
    }

    await tx
      .update(orders)
      .set({ status: "paid" })
      .where(eq(orders.id, orderId));

    return orderId;
  });
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

  return (await response.json()) as {
    id: number;
    status: string;
    external_reference?: string;
  };
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
    signature.split(",").map((part) => part.split("=") as [string, string]),
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
  const { db } = await import("@lindaflor/db");
  const { orders } = await import("@lindaflor/db/schema/commerce");
  const { sql } = await import("drizzle-orm");

  const [order] = await db
    .select({ id: orders.id })
    .from(orders)
    .where(
      sql`${orders.payment_meta}->>'external_id' = ${externalId}`,
    )
    .limit(1);

  if (!order) {
    return null;
  }

  return confirmOrderPayment(order.id);
}
