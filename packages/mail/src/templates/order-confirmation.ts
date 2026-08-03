import { env } from "@lindaflor/env/server";
import { mailer } from "@lindaflor/mail";

type OrderEmailItem = {
  product_name: string;
  variant_label: string;
  quantity: number;
  unit_price_cents: number;
};

type OrderEmail = {
  id: string;
  total_cents: number;
  items: OrderEmailItem[];
};

function escapeHtml(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

const brlFormatter = new Intl.NumberFormat("pt-BR", {
  style: "currency",
  currency: "BRL",
});

function formatBrl(cents: number) {
  return brlFormatter.format(cents / 100);
}

export function orderConfirmationEmailHtml(params: {
  order: OrderEmail;
  orderUrl: string;
}) {
  const { order, orderUrl } = params;
  const itemsHtml = order.items
    .map(
      (item: OrderEmailItem) =>
        `<tr><td style="padding:8px 0;border-bottom:1px solid #eee;">${escapeHtml(item.product_name)} · ${escapeHtml(item.variant_label)} × ${item.quantity}</td><td style="padding:8px 0;border-bottom:1px solid #eee;text-align:right;">${formatBrl(item.unit_price_cents * item.quantity)}</td></tr>`,
    )
    .join("");

  return `<!DOCTYPE html>
<html lang="pt-BR">
<head><meta charset="utf-8"><title>Pedido Linda Flor</title></head>
<body style="margin:0;padding:0;font-family:Georgia,serif;background:#faf7f2;">
  <table width="100%" cellpadding="0" cellspacing="0" style="background:#faf7f2;">
    <tr><td align="center" style="padding:32px 16px;">
      <table width="100%" style="max-width:520px;background:#fff;border:1px solid #eee;">
        <tr><td style="padding:32px 24px;">
          <p style="margin:0 0 8px;font-size:11px;letter-spacing:0.18em;text-transform:uppercase;color:#c97b8b;">Linda Flor Moda Praia</p>
          <h1 style="margin:0 0 16px;font-size:28px;font-weight:normal;">Pedido recebido!</h1>
          <p style="margin:0 0 24px;font-size:14px;line-height:1.6;color:#555;">Obrigada pela compra. Seu pedido <strong>#${order.id.slice(0, 8)}</strong> foi registrado. Pague via PIX para confirmarmos.</p>
          <table width="100%" style="font-size:14px;color:#333;">${itemsHtml}</table>
          <p style="margin:16px 0 0;font-size:14px;text-align:right;"><strong>Total: ${formatBrl(order.total_cents)}</strong></p>
          <p style="margin:24px 0;"><a href="${escapeHtml(orderUrl)}" style="display:inline-block;padding:12px 20px;background:#e8a0b0;color:#fff;text-decoration:none;font-size:12px;letter-spacing:0.14em;text-transform:uppercase;">Ver pedido e pagar</a></p>
        </td></tr>
      </table>
    </td></tr>
  </table>
</body></html>`;
}

export async function sendOrderConfirmationEmail(params: {
  to: string;
  order: OrderEmail;
  orderUrl: string;
}) {
  await mailer.emails.send({
    from: env.MAIL_FROM,
    to: params.to,
    subject: `Pedido Linda Flor #${params.order.id.slice(0, 8)}`,
    html: orderConfirmationEmailHtml(params),
  });
}
