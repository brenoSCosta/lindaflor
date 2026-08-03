import {
  confirmOrderPayment,
  handleMercadoPagoNotification,
  verifyMercadoPagoWebhookSignature,
} from "@lindaflor/core/commerce/payments";
import { z } from "zod";

const mercadoPagoWebhookBodySchema = z.object({
  type: z.string().optional(),
  action: z.string().optional(),
  data: z
    .object({
      id: z.union([z.string(), z.number()]).optional(),
    })
    .optional(),
});

export async function handleMercadoPagoWebhookRequest(
  request: Request,
): Promise<Response> {
  const url = new URL(request.url);
  const topic = url.searchParams.get("topic");
  const queryId = url.searchParams.get("id");

  if (topic === "payment" && queryId) {
    const result = await handleMercadoPagoNotification(queryId);
    return Response.json(result);
  }

  if (request.method === "POST") {
    const bodyText = await request.text();
    if (!verifyMercadoPagoWebhookSignature(request, bodyText)) {
      return new Response("Invalid signature", { status: 401 });
    }

    let parsedBody: z.infer<typeof mercadoPagoWebhookBodySchema>;
    try {
      const json: unknown = JSON.parse(bodyText);
      const parsed = mercadoPagoWebhookBodySchema.safeParse(json);
      if (!parsed.success) {
        return new Response("Invalid JSON", { status: 400 });
      }
      parsedBody = parsed.data;
    } catch {
      return new Response("Invalid JSON", { status: 400 });
    }

    if (parsedBody.type === "payment" && parsedBody.data?.id != null) {
      const result = await handleMercadoPagoNotification(
        String(parsedBody.data.id),
      );
      return Response.json(result);
    }
  }

  return Response.json({ ok: true, ignored: true });
}

export async function handleDevPaymentConfirm(orderId: string) {
  const confirmed = await confirmOrderPayment(orderId);
  if (!confirmed) {
    return new Response("Order not found or already paid", { status: 404 });
  }
  return Response.json({ ok: true, orderId: confirmed });
}
