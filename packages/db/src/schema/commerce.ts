import { relations } from "drizzle-orm";
import {
  boolean,
  index,
  integer,
  jsonb,
  pgEnum,
  pgTable,
  text,
  timestamp,
  uniqueIndex,
  uuid,
} from "drizzle-orm/pg-core";
import { v7 as uuidv7 } from "uuid";

import { users } from "@lindaflor/db/schema/auth";

export const product_category = pgEnum("product_category", [
  "biquini",
  "maio",
  "saida_praia",
  "acessorio",
]);

export const product_size = pgEnum("product_size", [
  "pp",
  "p",
  "m",
  "g",
  "gg",
]);

export const order_status = pgEnum("order_status", [
  "pending_payment",
  "paid",
  "processing",
  "shipped",
  "delivered",
  "cancelled",
]);

export const inventory_movement_type = pgEnum("inventory_movement_type", [
  "entrada",
  "saida",
  "reserva",
  "liberacao",
  "ajuste",
  "venda",
  "transferencia",
]);

export const warehouses = pgTable(
  "warehouses",
  {
    id: uuid("id")
      .primaryKey()
      .$defaultFn(() => uuidv7()),
    code: text("code").notNull(),
    name: text("name").notNull(),
    is_default: boolean("is_default").default(false).notNull(),
    active: boolean("active").default(true).notNull(),
    created_at: timestamp("created_at").defaultNow().notNull(),
  },
  (table) => [
    uniqueIndex("warehouses_code_uidx").on(table.code),
    index("warehouses_is_default_idx").on(table.is_default),
  ],
);
export type Warehouse = typeof warehouses.$inferSelect;

export const collections = pgTable(
  "collections",
  {
    id: uuid("id")
      .primaryKey()
      .$defaultFn(() => uuidv7()),
    name: text("name").notNull(),
    slug: text("slug").notNull(),
    description: text("description"),
    active: boolean("active").default(true).notNull(),
    created_at: timestamp("created_at").defaultNow().notNull(),
    updated_at: timestamp("updated_at")
      .defaultNow()
      .$onUpdate(() => /* @__PURE__ */ new Date())
      .notNull(),
  },
  (table) => [uniqueIndex("collections_slug_uidx").on(table.slug)],
);
export type Collection = typeof collections.$inferSelect;

export const products = pgTable(
  "products",
  {
    id: uuid("id")
      .primaryKey()
      .$defaultFn(() => uuidv7()),
    name: text("name").notNull(),
    slug: text("slug").notNull(),
    description: text("description"),
    price_in_cents: integer("price_in_cents").notNull(),
    category: product_category("category").notNull().default("biquini"),
    collection_id: uuid("collection_id").references(() => collections.id, {
      onDelete: "set null",
    }),
    active: boolean("active").default(true).notNull(),
    featured: boolean("featured").default(false).notNull(),
    created_at: timestamp("created_at").defaultNow().notNull(),
    updated_at: timestamp("updated_at")
      .defaultNow()
      .$onUpdate(() => /* @__PURE__ */ new Date())
      .notNull(),
  },
  (table) => [
    uniqueIndex("products_slug_uidx").on(table.slug),
    index("products_collection_id_idx").on(table.collection_id),
    index("products_active_idx").on(table.active),
  ],
);
export type Product = typeof products.$inferSelect;

export const product_images = pgTable(
  "product_images",
  {
    id: uuid("id")
      .primaryKey()
      .$defaultFn(() => uuidv7()),
    product_id: uuid("product_id")
      .notNull()
      .references(() => products.id, { onDelete: "cascade" }),
    url: text("url").notNull(),
    alt: text("alt"),
    sort_order: integer("sort_order").default(0).notNull(),
    created_at: timestamp("created_at").defaultNow().notNull(),
  },
  (table) => [index("product_images_product_id_idx").on(table.product_id)],
);
export type ProductImage = typeof product_images.$inferSelect;

export const product_variants = pgTable(
  "product_variants",
  {
    id: uuid("id")
      .primaryKey()
      .$defaultFn(() => uuidv7()),
    product_id: uuid("product_id")
      .notNull()
      .references(() => products.id, { onDelete: "cascade" }),
    sku: text("sku").notNull(),
    size: product_size("size").notNull(),
    color: text("color").notNull(),
    price_in_cents: integer("price_in_cents"),
    low_stock_threshold: integer("low_stock_threshold").notNull().default(5),
    created_at: timestamp("created_at").defaultNow().notNull(),
    updated_at: timestamp("updated_at")
      .defaultNow()
      .$onUpdate(() => /* @__PURE__ */ new Date())
      .notNull(),
  },
  (table) => [
    uniqueIndex("product_variants_sku_uidx").on(table.sku),
    index("product_variants_product_id_idx").on(table.product_id),
  ],
);
export type ProductVariant = typeof product_variants.$inferSelect;

export const inventory = pgTable(
  "inventory",
  {
    id: uuid("id")
      .primaryKey()
      .$defaultFn(() => uuidv7()),
    variant_id: uuid("variant_id")
      .notNull()
      .references(() => product_variants.id, { onDelete: "cascade" }),
    warehouse_id: uuid("warehouse_id")
      .notNull()
      .references(() => warehouses.id, { onDelete: "restrict" }),
    quantity: integer("quantity").notNull().default(0),
    reserved: integer("reserved").notNull().default(0),
    updated_at: timestamp("updated_at")
      .defaultNow()
      .$onUpdate(() => /* @__PURE__ */ new Date())
      .notNull(),
  },
  (table) => [
    uniqueIndex("inventory_variant_warehouse_uidx").on(
      table.variant_id,
      table.warehouse_id,
    ),
    index("inventory_warehouse_id_idx").on(table.warehouse_id),
  ],
);
export type Inventory = typeof inventory.$inferSelect;

export const inventory_movements = pgTable(
  "inventory_movements",
  {
    id: uuid("id")
      .primaryKey()
      .$defaultFn(() => uuidv7()),
    variant_id: uuid("variant_id")
      .notNull()
      .references(() => product_variants.id, { onDelete: "cascade" }),
    warehouse_id: uuid("warehouse_id").references(() => warehouses.id, {
      onDelete: "set null",
    }),
    type: inventory_movement_type("type").notNull(),
    quantity: integer("quantity").notNull(),
    reference_type: text("reference_type"),
    reference_id: uuid("reference_id"),
    notes: text("notes"),
    created_by: uuid("created_by").references(() => users.id, {
      onDelete: "set null",
    }),
    created_at: timestamp("created_at").defaultNow().notNull(),
  },
  (table) => [
    index("inventory_movements_variant_id_idx").on(table.variant_id),
    index("inventory_movements_created_at_idx").on(table.created_at),
  ],
);
export type InventoryMovement = typeof inventory_movements.$inferSelect;

export const orders = pgTable(
  "orders",
  {
    id: uuid("id")
      .primaryKey()
      .$defaultFn(() => uuidv7()),
    user_id: uuid("user_id").references(() => users.id, {
      onDelete: "set null",
    }),
    guest_email: text("guest_email"),
    status: order_status("status").notNull().default("pending_payment"),
    subtotal_cents: integer("subtotal_cents").notNull().default(0),
    shipping_cents: integer("shipping_cents").notNull().default(0),
    discount_cents: integer("discount_cents").notNull().default(0),
    total_cents: integer("total_cents").notNull().default(0),
    shipping_address: jsonb("shipping_address").$type<{
      name: string;
      street: string;
      number: string;
      complement?: string;
      neighborhood: string;
      city: string;
      state: string;
      zip_code: string;
    }>(),
    notes: text("notes"),
    payment_meta: jsonb("payment_meta").$type<{
      provider?: "mercado_pago" | "manual";
      external_id?: string;
      pix_copy_paste?: string;
      pix_qr_base64?: string;
      ticket_url?: string;
    }>(),
    created_at: timestamp("created_at").defaultNow().notNull(),
    updated_at: timestamp("updated_at")
      .defaultNow()
      .$onUpdate(() => /* @__PURE__ */ new Date())
      .notNull(),
  },
  (table) => [
    index("orders_user_id_idx").on(table.user_id),
    index("orders_status_idx").on(table.status),
    index("orders_created_at_idx").on(table.created_at),
  ],
);
export type Order = typeof orders.$inferSelect;

export const order_items = pgTable(
  "order_items",
  {
    id: uuid("id")
      .primaryKey()
      .$defaultFn(() => uuidv7()),
    order_id: uuid("order_id")
      .notNull()
      .references(() => orders.id, { onDelete: "cascade" }),
    variant_id: uuid("variant_id")
      .notNull()
      .references(() => product_variants.id, { onDelete: "restrict" }),
    product_name: text("product_name").notNull(),
    variant_label: text("variant_label").notNull(),
    quantity: integer("quantity").notNull(),
    unit_price_cents: integer("unit_price_cents").notNull(),
    created_at: timestamp("created_at").defaultNow().notNull(),
  },
  (table) => [index("order_items_order_id_idx").on(table.order_id)],
);
export type OrderItem = typeof order_items.$inferSelect;

export const collections_relations = relations(collections, ({ many }) => ({
  products: many(products),
}));

export const products_relations = relations(products, ({ one, many }) => ({
  collection: one(collections, {
    fields: [products.collection_id],
    references: [collections.id],
  }),
  images: many(product_images),
  variants: many(product_variants),
}));

export const product_images_relations = relations(product_images, ({ one }) => ({
  product: one(products, {
    fields: [product_images.product_id],
    references: [products.id],
  }),
}));

export const product_variants_relations = relations(
  product_variants,
  ({ one }) => ({
    product: one(products, {
      fields: [product_variants.product_id],
      references: [products.id],
    }),
    inventory: one(inventory, {
      fields: [product_variants.id],
      references: [inventory.variant_id],
    }),
  }),
);

export const inventory_relations = relations(inventory, ({ one }) => ({
  variant: one(product_variants, {
    fields: [inventory.variant_id],
    references: [product_variants.id],
  }),
  warehouse: one(warehouses, {
    fields: [inventory.warehouse_id],
    references: [warehouses.id],
  }),
}));

export const warehouses_relations = relations(warehouses, ({ many }) => ({
  inventory: many(inventory),
}));

export const orders_relations = relations(orders, ({ one, many }) => ({
  user: one(users, {
    fields: [orders.user_id],
    references: [users.id],
  }),
  items: many(order_items),
}));

export const order_items_relations = relations(order_items, ({ one }) => ({
  order: one(orders, {
    fields: [order_items.order_id],
    references: [orders.id],
  }),
  variant: one(product_variants, {
    fields: [order_items.variant_id],
    references: [product_variants.id],
  }),
}));
