/**
 * Bacen PIX EMV (copia e cola) payload generator.
 * Spec: Manual de Padrões para Iniciação do PIX (BR Code).
 */

function emvField(id: string, value: string) {
  const length = value.length.toString().padStart(2, "0");
  return `${id}${length}${value}`;
}

function crc16CcittFalse(payload: string) {
  let crc = 0xffff;
  for (let i = 0; i < payload.length; i++) {
    crc ^= payload.charCodeAt(i) << 8;
    for (let bit = 0; bit < 8; bit++) {
      if ((crc & 0x8000) !== 0) {
        crc = ((crc << 1) ^ 0x1021) & 0xffff;
      } else {
        crc = (crc << 1) & 0xffff;
      }
    }
  }
  return crc.toString(16).toUpperCase().padStart(4, "0");
}

function sanitizeMerchantField(value: string, maxLength: number) {
  return value
    .normalize("NFD")
    .replace(/\p{M}/gu, "")
    .replace(/[^A-Za-z0-9 ]/g, "")
    .trim()
    .slice(0, maxLength)
    .toUpperCase();
}

function normalizePixKey(key: string, keyType: string) {
  const trimmed = key.trim();
  if (keyType === "cpf" || keyType === "cnpj" || keyType === "phone") {
    return trimmed.replace(/\D/g, "");
  }
  if (keyType === "email") {
    return trimmed.toLowerCase();
  }
  return trimmed;
}

function formatAmountCents(amountCents: number) {
  return (amountCents / 100).toFixed(2);
}

/** PIX txid: alphanumeric, 1–25 chars. */
function toTxid(orderId: string) {
  return orderId.replace(/-/g, "").slice(0, 25);
}

export function generateStaticPixPayload(params: {
  pix_key: string;
  pix_key_type: string;
  merchant_name: string;
  merchant_city: string;
  amount_cents: number;
  order_id: string;
}): string {
  const key = normalizePixKey(params.pix_key, params.pix_key_type);
  const merchantName = sanitizeMerchantField(params.merchant_name, 25);
  const merchantCity = sanitizeMerchantField(params.merchant_city, 15);
  const txid = toTxid(params.order_id);

  if (!key || !merchantName || !merchantCity) {
    throw new Error("Dados PIX incompletos");
  }

  const merchantAccount =
    emvField("00", "br.gov.bcb.pix") + emvField("01", key);

  const additionalData = emvField("05", txid);

  const payloadWithoutCrc =
    emvField("00", "01") +
    emvField("26", merchantAccount) +
    emvField("52", "0000") +
    emvField("53", "986") +
    emvField("54", formatAmountCents(params.amount_cents)) +
    emvField("58", "BR") +
    emvField("59", merchantName) +
    emvField("60", merchantCity) +
    emvField("62", additionalData) +
    "6304";

  return payloadWithoutCrc + crc16CcittFalse(payloadWithoutCrc);
}

export function buildWhatsAppUrl(params: { phone: string; message: string }) {
  const digits = params.phone.replace(/\D/g, "");
  if (!digits) {
    return null;
  }
  return `https://wa.me/${digits}?text=${encodeURIComponent(params.message)}`;
}

export function renderWhatsAppTemplate(
  template: string | null | undefined,
  vars: { order_id: string; total: string },
) {
  const base =
    template?.trim() ||
    "Olá! Fiz o pedido {{order_id}} no valor de {{total}} e quero confirmar o pagamento via PIX.";
  return base
    .replaceAll("{{order_id}}", vars.order_id.slice(0, 8))
    .replaceAll("{{total}}", vars.total);
}
