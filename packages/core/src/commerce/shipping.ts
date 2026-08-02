import { env } from "@lindaflor/env/server";
import type { ShippingAddress } from "@lindaflor/shared/schemas/commerce";

export function resolveShippingState(params: {
  state: string;
  zip_code?: string;
}) {
  const state = params.state.toUpperCase();
  if (state.length === 2) {
    return state;
  }

  const zip = params.zip_code?.replace(/\D/g, "") ?? "";
  if (zip.startsWith("49")) {
    return "SE";
  }

  return state;
}

export function calculateShippingCents(params: {
  state: string;
  zip_code?: string;
  subtotal_cents: number;
}) {
  const region = resolveShippingState(params);
  const FREE_SHIPPING_THRESHOLD_CENTS = 29_900;
  const SHIPPING_SE_CENTS = 1_990;
  const SHIPPING_DEFAULT_CENTS = 3_990;
  const SHIPPING_REMOTE_CENTS = 5_990;

  const zip = params.zip_code?.replace(/\D/g, "") ?? "";
  const isRemote =
    zip.length >= 5 &&
    !zip.startsWith("49") &&
    (zip.startsWith("69") ||
      zip.startsWith("68") ||
      zip.startsWith("66") ||
      zip.startsWith("78") ||
      zip.startsWith("79"));

  const free_shipping = params.subtotal_cents >= FREE_SHIPPING_THRESHOLD_CENTS;
  if (free_shipping) {
    return {
      shipping_cents: 0,
      free_shipping: true,
      estimated_days:
        region === "SE" ? "3 a 7 dias úteis" : "5 a 12 dias úteis",
    };
  }

  const shipping_cents =
    region === "SE"
      ? SHIPPING_SE_CENTS
      : isRemote
        ? SHIPPING_REMOTE_CENTS
        : SHIPPING_DEFAULT_CENTS;

  return {
    shipping_cents,
    free_shipping: false,
    estimated_days:
      region === "SE" ? "3 a 7 dias úteis" : "7 a 15 dias úteis",
  };
}

export function calculateCouponDiscountCents(params: {
  subtotal_cents: number;
  coupon_code?: string;
}) {
  if (!params.coupon_code?.trim() || !env.STORE_COUPON_CODE) {
    return 0;
  }

  if (
    params.coupon_code.trim().toUpperCase() !==
    env.STORE_COUPON_CODE.toUpperCase()
  ) {
    return 0;
  }

  return Math.floor(
    (params.subtotal_cents * env.STORE_COUPON_DISCOUNT_PERCENT) / 100,
  );
}

export function normalizeZipCode(zip: string) {
  return zip.replace(/\D/g, "");
}

export function formatShippingAddress(address: ShippingAddress) {
  return `${address.street}, ${address.number}${
    address.complement ? ` — ${address.complement}` : ""
  }, ${address.neighborhood}, ${address.city}/${address.state} — CEP ${address.zip_code}`;
}
