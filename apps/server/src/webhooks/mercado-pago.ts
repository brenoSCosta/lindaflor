import {
  confirmOrderPayment,
  handleMercadoPagoNotification,
  verifyMercadoPagoWebhookSignature,
} from "@lindaflor/core/commerce/payments";

type MercadoPagoWebhookBody = {
  type?: string;
  action?: string;
  data?: { id?: string | number };
};

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

    let body: MercadoPagoWebhookBody;
    try {
      body = JSON.parse(bodyText) as MercadoPagoWebhookBody;
    } catch {
      return new Response("Invalid JSON", { status: 400 });
    }

    if (body.type === "payment" && body.data?.id != null) {
      const result = await handleMercadoPagoNotification(String(body.data.id));
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
