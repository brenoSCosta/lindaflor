import { describe, expect, it } from "bun:test";

import {
  buildWhatsAppUrl,
  generateStaticPixPayload,
  renderWhatsAppTemplate,
} from "@lindaflor/core/commerce/pix-static";

describe("generateStaticPixPayload", () => {
  it("generates a valid EMV payload with CRC", () => {
    const payload = generateStaticPixPayload({
      pix_key: "11999999999",
      pix_key_type: "phone",
      merchant_name: "Linda Flor",
      merchant_city: "Aracaju",
      amount_cents: 19990,
      order_id: "019abcdef0123456789abcdef0",
    });

    expect(payload.startsWith("000201")).toBe(true);
    expect(payload.includes("br.gov.bcb.pix")).toBe(true);
    expect(payload.includes("11999999999")).toBe(true);
    expect(payload.includes("5406199.90")).toBe(true);
    expect(payload.includes("5802BR")).toBe(true);
    expect(payload.includes("5910LINDA FLOR")).toBe(true);
    expect(payload.includes("6007ARACAJU")).toBe(true);
    expect(payload.slice(-4)).toMatch(/^[0-9A-F]{4}$/);
    expect(payload.length).toBeGreaterThan(50);
  });

  it("normalizes CPF digits and strips accents from merchant fields", () => {
    const payload = generateStaticPixPayload({
      pix_key: "123.456.789-09",
      pix_key_type: "cpf",
      merchant_name: "José Açúcar",
      merchant_city: "São Paulo",
      amount_cents: 100,
      order_id: "abc-123",
    });

    expect(payload.includes("12345678909")).toBe(true);
    expect(payload.includes("JOSE ACUCAR")).toBe(true);
    expect(payload.includes("SAO PAULO")).toBe(true);
  });
});

describe("whatsapp helpers", () => {
  it("builds wa.me url", () => {
    expect(
      buildWhatsAppUrl({ phone: "+55 (79) 99999-9999", message: "oi" }),
    ).toBe("https://wa.me/5579999999999?text=oi");
  });

  it("renders template placeholders", () => {
    expect(
      renderWhatsAppTemplate("Pedido {{order_id}} — {{total}}", {
        order_id: "abcdef12-3456",
        total: "R$ 10,00",
      }),
    ).toBe("Pedido abcdef12 — R$ 10,00");
  });
});
