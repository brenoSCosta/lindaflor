import { allocateFulfillment } from "@lindaflor/core/commerce/fulfillment";
import { reserveFulfillmentStock } from "@lindaflor/core/commerce/inventory";
import { createPixPayment } from "@lindaflor/core/commerce/payments";
import { resolveImageUrl } from "@lindaflor/core/commerce/product-images";
import { formatVariantLabel } from "@lindaflor/core/commerce/products";
import {
  calculateCouponDiscountCents,
  calculateShippingCents,
} from "@lindaflor/core/commerce/shipping";
import { db } from "@lindaflor/db";
import {
  order_items,
  orders,
  product_images,
  product_variants,
  products,
  warehouses,
} from "@lindaflor/db/schema/commerce";
import { env } from "@lindaflor/env/server";
import { sendOrderConfirmationEmail } from "@lindaflor/mail/templates/order-confirmation";
import { PRODUCTS_IMAGES_PREFIX, deleteFile, uploadFile } from "@lindaflor/s3";
import { schema } from "@lindaflor/shared/schemas/commerce";
import { ORPCError } from "@orpc/server";
import { eq, inArray } from "drizzle-orm";
import { Effect } from "effect";
import { v7 as uuidv7 } from "uuid";
import type { z } from "zod";

type CreateOrderInput = z.infer<typeof schema.store.createOrder.input>;

function reservationExpiresAt(createdAt: Date) {
  return new Date(
    createdAt.getTime() + env.ORDER_RESERVATION_HOURS * 60 * 60 * 1000,
  );
}

async function mapOrder(orderId: string) {
  const [order] = await db
    .select()
    .from(orders)
    .where(eq(orders.id, orderId))
    .limit(1);

  if (!order) {
    throw new ORPCError("NOT_FOUND", { message: "Pedido não encontrado" });
  }

  const items = await db
    .select({
      id: order_items.id,
      product_name: order_items.product_name,
      variant_label: order_items.variant_label,
      quantity: order_items.quantity,
      unit_price_cents: order_items.unit_price_cents,
      warehouse_id: order_items.warehouse_id,
      warehouse_code: warehouses.code,
      warehouse_name: warehouses.name,
    })
    .from(order_items)
    .leftJoin(warehouses, eq(warehouses.id, order_items.warehouse_id))
    .where(eq(order_items.order_id, orderId));

  return schema.store.getOrder.output.parse({
    id: order.id,
    status: order.status,
    guest_email: order.guest_email,
    subtotal_cents: order.subtotal_cents,
    shipping_cents: order.shipping_cents,
    discount_cents: order.discount_cents,
    total_cents: order.total_cents,
    shipping_address: order.shipping_address,
    payment_meta: order.payment_meta ?? null,
    items: items.map((item) => ({
      id: item.id,
      product_name: item.product_name,
      variant_label: item.variant_label,
      quantity: item.quantity,
      unit_price_cents: item.unit_price_cents,
      warehouse_id: item.warehouse_id,
      warehouse_code: item.warehouse_code,
      warehouse_name: item.warehouse_name,
    })),
    created_at: order.created_at,
    reservation_expires_at:
      order.status === "pending_payment"
        ? reservationExpiresAt(order.created_at)
        : null,
  });
}

export async function getStoreOrder(orderId: string) {
  return mapOrder(orderId);
}

export async function createStoreOrder(
  input: CreateOrderInput,
  userId?: string | null,
) {
  const variantIds = input.items.map((item) => item.variant_id);

  const orderId = await db.transaction(async (tx) => {
    const variantRows = await tx
      .select({
        id: product_variants.id,
        sku: product_variants.sku,
        size: product_variants.size,
        color: product_variants.color,
        price_in_cents: product_variants.price_in_cents,
        product_name: products.name,
        product_price: products.price_in_cents,
      })
      .from(product_variants)
      .innerJoin(products, eq(products.id, product_variants.product_id))
      .where(inArray(product_variants.id, variantIds));

    if (variantRows.length !== input.items.length) {
      throw new ORPCError("BAD_REQUEST", {
        message: "Uma ou mais variantes são inválidas",
      });
    }

    const variantById = new Map(variantRows.map((row) => [row.id, row]));
    let subtotal_cents = 0;

    for (const item of input.items) {
      const variant = variantById.get(item.variant_id);
      if (!variant) {
        throw new ORPCError("BAD_REQUEST", {
          message: "Variante não encontrada",
        });
      }
      const unitPrice = variant.price_in_cents ?? variant.product_price;
      subtotal_cents += unitPrice * item.quantity;
    }

    const allocation = await allocateFulfillment(input.items, tx);

    const discount_cents = calculateCouponDiscountCents({
      subtotal_cents,
      coupon_code: input.coupon_code,
    });

    if (
      input.coupon_code?.trim() &&
      env.STORE_COUPON_CODE &&
      discount_cents === 0
    ) {
      throw new ORPCError("BAD_REQUEST", { message: "Cupom inválido" });
    }

    const shipping = calculateShippingCents({
      state: input.shipping_address.state,
      zip_code: input.shipping_address.zip_code,
      subtotal_cents,
    });

    const total_cents =
      subtotal_cents + shipping.shipping_cents - discount_cents;

    const [created] = await tx
      .insert(orders)
      .values({
        user_id: userId ?? null,
        guest_email: input.guest_email,
        status: "pending_payment",
        subtotal_cents,
        shipping_cents: shipping.shipping_cents,
        discount_cents,
        total_cents,
        shipping_address: input.shipping_address,
        notes: input.notes ?? null,
      })
      .returning();

    if (!created) {
      throw new ORPCError("INTERNAL_SERVER_ERROR", {
        message: "Falha ao criar pedido",
      });
    }

    await Promise.all(
      allocation.map(async (line) => {
        const variant = variantById.get(line.variant_id);
        if (!variant) {
          throw new ORPCError("BAD_REQUEST", {
            message: "Variante não encontrada",
          });
        }

        const unitPrice = variant.price_in_cents ?? variant.product_price;

        await tx.insert(order_items).values({
          order_id: created.id,
          variant_id: variant.id,
          warehouse_id: line.warehouse_id,
          product_name: variant.product_name,
          variant_label: formatVariantLabel(variant.size, variant.color),
          quantity: line.quantity,
          unit_price_cents: unitPrice,
        });

        await reserveFulfillmentStock(
          {
            variant_id: line.variant_id,
            quantity: line.quantity,
            order_id: created.id,
            warehouse_id: line.warehouse_id,
          },
          tx,
        );
      }),
    );

    const payment_meta = await createPixPayment({
      orderId: created.id,
      total_cents: created.total_cents,
      guest_email: input.guest_email,
      description: `Pedido Linda Flor ${created.id.slice(0, 8)}`,
    });

    await tx
      .update(orders)
      .set({ payment_meta })
      .where(eq(orders.id, created.id));

    return created.id;
  });

  const orderResult = await mapOrder(orderId);

  await Effect.runPromise(
    Effect.tryPromise({
      try: () =>
        sendOrderConfirmationEmail({
          to: input.guest_email,
          order: orderResult,
          orderUrl: `${env.WEB_ORIGIN}/pedido/${orderId}`,
        }),
      catch: () => undefined,
    }).pipe(Effect.orElseSucceed(() => undefined)),
  );

  return orderResult;
}

type UploadProductImageInput = z.infer<
  typeof schema.admin.uploadProductImage.input
>;

function extensionFromMimeType(mimeType: string) {
  switch (mimeType) {
    case "image/jpeg":
      return "jpg";
    case "image/png":
      return "png";
    case "image/webp":
      return "webp";
    default:
      return "jpg";
  }
}

export async function uploadProductImage(input: UploadProductImageInput) {
  const [product] = await db
    .select({ id: products.id })
    .from(products)
    .where(eq(products.id, input.product_id))
    .limit(1);

  if (!product) {
    throw new ORPCError("NOT_FOUND", { message: "Produto não encontrado" });
  }

  const existingImages = await db
    .select({ sort_order: product_images.sort_order })
    .from(product_images)
    .where(eq(product_images.product_id, input.product_id));

  const sort_order =
    existingImages.reduce((max, img) => Math.max(max, img.sort_order), -1) + 1;

  const fileKey = `${PRODUCTS_IMAGES_PREFIX}/${input.product_id}/${uuidv7()}.${extensionFromMimeType(input.file.type)}`;
  const buffer = Buffer.from(await input.file.arrayBuffer());

  await Effect.runPromise(
    Effect.tryPromise({
      try: () => uploadFile(fileKey, buffer, input.file.type),
      catch: () =>
        new ORPCError("SERVICE_UNAVAILABLE", {
          message: "Falha ao enviar imagem",
        }),
    }),
  );

  const [created] = await db
    .insert(product_images)
    .values({
      product_id: input.product_id,
      url: fileKey,
      alt: input.alt ?? null,
      sort_order,
    })
    .returning();

  if (!created) {
    throw new ORPCError("INTERNAL_SERVER_ERROR", {
      message: "Falha ao salvar imagem",
    });
  }

  const url = await resolveImageUrl(created.url);

  return schema.admin.uploadProductImage.output.parse({
    id: created.id,
    url,
    alt: created.alt,
    sort_order: created.sort_order,
  });
}

type DeleteProductImageInput = z.infer<
  typeof schema.admin.deleteProductImage.input
>;

export async function deleteProductImage(input: DeleteProductImageInput) {
  const [image] = await db
    .select()
    .from(product_images)
    .where(eq(product_images.id, input.id))
    .limit(1);

  if (!image) {
    throw new ORPCError("NOT_FOUND", { message: "Imagem não encontrada" });
  }

  if (!image.url.includes("://")) {
    await Effect.runPromise(
      Effect.tryPromise({
        try: () => deleteFile(image.url),
        catch: () => undefined,
      }).pipe(Effect.orElseSucceed(() => undefined)),
    );
  }

  await db.delete(product_images).where(eq(product_images.id, input.id));

  return schema.admin.deleteProductImage.output.parse({ success: true });
}
