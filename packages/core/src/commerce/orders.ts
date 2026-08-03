import { releaseFulfillmentStock } from "@lindaflor/core/commerce/inventory";
import { db } from "@lindaflor/db";
import { order_items, orders } from "@lindaflor/db/schema/commerce";
import type { orderStatuses } from "@lindaflor/shared/enums/commerce";
import { schema } from "@lindaflor/shared/schemas/commerce";
import { ORPCError } from "@orpc/server";
import { and, desc, eq, lt, or, sql } from "drizzle-orm";
import type { z } from "zod";

type OrderStatus = (typeof orderStatuses)[number];

const STATUS_TRANSITIONS: Record<OrderStatus, OrderStatus[]> = {
  pending_payment: ["paid", "cancelled"],
  paid: ["processing", "cancelled"],
  processing: ["shipped", "cancelled"],
  shipped: ["delivered"],
  delivered: [],
  cancelled: [],
};

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
    .select()
    .from(order_items)
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
    })),
    created_at: order.created_at,
  });
}

export async function listOrders() {
  const rows = await db
    .select({
      id: orders.id,
      status: orders.status,
      guest_email: orders.guest_email,
      total_cents: orders.total_cents,
      created_at: orders.created_at,
      item_count: sql<number>`(
        SELECT COALESCE(SUM(oi.quantity), 0)::int
        FROM order_items oi
        WHERE oi.order_id = ${orders.id}
      )`,
    })
    .from(orders)
    .orderBy(desc(orders.created_at));

  return { data: schema.admin.listOrders.output.shape.data.parse(rows) };
}

export async function getAdminOrder(orderId: string) {
  return mapOrder(orderId);
}

export async function listCustomerOrders(params: {
  userId: string;
  email: string;
}) {
  const rows = await db
    .select({
      id: orders.id,
      status: orders.status,
      guest_email: orders.guest_email,
      total_cents: orders.total_cents,
      created_at: orders.created_at,
      item_count: sql<number>`(
        SELECT COALESCE(SUM(oi.quantity), 0)::int
        FROM order_items oi
        WHERE oi.order_id = ${orders.id}
      )`,
    })
    .from(orders)
    .where(
      or(
        eq(orders.user_id, params.userId),
        eq(orders.guest_email, params.email),
      ),
    )
    .orderBy(desc(orders.created_at));

  return { data: schema.store.listMyOrders.output.shape.data.parse(rows) };
}

type UpdateOrderStatusInput = z.infer<
  typeof schema.admin.updateOrderStatus.input
>;

async function releaseOrderReservations(orderId: string) {
  const items = await db
    .select()
    .from(order_items)
    .where(eq(order_items.order_id, orderId));

  await Promise.all(
    items.map((item) =>
      releaseFulfillmentStock({
        variant_id: item.variant_id,
        quantity: item.quantity,
        order_id: orderId,
      }),
    ),
  );
}

export async function updateOrderStatus(input: UpdateOrderStatusInput) {
  const [order] = await db
    .select()
    .from(orders)
    .where(eq(orders.id, input.id))
    .limit(1);

  if (!order) {
    throw new ORPCError("NOT_FOUND", { message: "Pedido não encontrado" });
  }

  const allowed = STATUS_TRANSITIONS[order.status];
  if (!allowed.includes(input.status)) {
    throw new ORPCError("BAD_REQUEST", {
      message: `Não é possível mudar de ${order.status} para ${input.status}`,
    });
  }

  if (input.status === "cancelled" && order.status === "pending_payment") {
    await releaseOrderReservations(order.id);
  }

  await db
    .update(orders)
    .set({ status: input.status })
    .where(eq(orders.id, order.id));

  return mapOrder(order.id);
}

export async function releaseExpiredReservations(hours = 24) {
  const cutoff = new Date(Date.now() - hours * 60 * 60 * 1000);

  const expired = await db
    .select({ id: orders.id })
    .from(orders)
    .where(
      and(eq(orders.status, "pending_payment"), lt(orders.created_at, cutoff)),
    );

  await Promise.all(
    expired.map(async (order) => {
      await releaseOrderReservations(order.id);
      await db
        .update(orders)
        .set({ status: "cancelled" })
        .where(eq(orders.id, order.id));
    }),
  );

  return expired.length;
}
