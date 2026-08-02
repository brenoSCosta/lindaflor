import {
  orderStatuses,
  productCategories,
  productSizes,
} from "@lindaflor/shared/enums/commerce";
import { z } from "zod";

const productImage = z.object({
  id: z.uuid(),
  url: z.string(),
  alt: z.string().nullable(),
  sort_order: z.number(),
});

const productVariant = z.object({
  id: z.uuid(),
  sku: z.string(),
  size: z.enum(productSizes),
  color: z.string(),
  price_in_cents: z.number().nullable(),
  available: z.number().int().nonnegative(),
});

const productListItem = z.object({
  id: z.uuid(),
  name: z.string(),
  slug: z.string(),
  description: z.string().nullable(),
  price_in_cents: z.number().int().positive(),
  category: z.enum(productCategories),
  featured: z.boolean(),
  image_url: z.string().nullable(),
  available_total: z.number().int().nonnegative(),
});

const productDetail = productListItem.extend({
  images: z.array(productImage),
  variants: z.array(productVariant),
  collection: z
    .object({
      id: z.uuid(),
      name: z.string(),
      slug: z.string(),
    })
    .nullable(),
});

const adminProduct = z.object({
  id: z.uuid(),
  name: z.string(),
  slug: z.string(),
  description: z.string().nullable(),
  price_in_cents: z.number().int().positive(),
  category: z.enum(productCategories),
  active: z.boolean(),
  featured: z.boolean(),
  variant_count: z.number().int().nonnegative(),
  available_total: z.number().int().nonnegative(),
  created_at: z.coerce.date(),
});

const inventoryItem = z.object({
  variant_id: z.uuid(),
  product_name: z.string(),
  sku: z.string(),
  size: z.enum(productSizes),
  color: z.string(),
  warehouse_id: z.uuid(),
  warehouse_code: z.string(),
  warehouse_name: z.string(),
  quantity: z.number().int(),
  reserved: z.number().int(),
  available: z.number().int(),
  low_stock_threshold: z.number().int(),
  is_low_stock: z.boolean(),
});

const inventoryMovement = z.object({
  id: z.uuid(),
  variant_id: z.uuid(),
  product_name: z.string(),
  sku: z.string(),
  warehouse_id: z.uuid().nullable(),
  warehouse_name: z.string().nullable(),
  type: z.enum([
    "entrada",
    "saida",
    "reserva",
    "liberacao",
    "ajuste",
    "venda",
    "transferencia",
  ]),
  quantity: z.number().int(),
  notes: z.string().nullable(),
  reference_type: z.string().nullable(),
  reference_id: z.uuid().nullable(),
  created_by_name: z.string().nullable(),
  created_at: z.coerce.date(),
});

const warehouseItem = z.object({
  id: z.uuid(),
  code: z.string(),
  name: z.string(),
  is_default: z.boolean(),
  active: z.boolean(),
});

const orderListItem = z.object({
  id: z.uuid(),
  status: z.enum(orderStatuses),
  guest_email: z.string().nullable(),
  total_cents: z.number().int(),
  item_count: z.number().int(),
  created_at: z.coerce.date(),
});

export const shippingAddressSchema = z.object({
  name: z.string().min(1),
  street: z.string().min(1),
  number: z.string().min(1),
  complement: z.string().optional(),
  neighborhood: z.string().min(1),
  city: z.string().min(1),
  state: z.string().length(2),
  zip_code: z.string().min(8).max(9),
});

const orderItemSnapshot = z.object({
  id: z.uuid(),
  product_name: z.string(),
  variant_label: z.string(),
  quantity: z.number().int(),
  unit_price_cents: z.number().int(),
});

const storeOrder = z.object({
  id: z.uuid(),
  status: z.enum(orderStatuses),
  guest_email: z.string().nullable(),
  subtotal_cents: z.number().int(),
  shipping_cents: z.number().int(),
  discount_cents: z.number().int(),
  total_cents: z.number().int(),
  shipping_address: shippingAddressSchema.nullable(),
  payment_meta: z
    .object({
      provider: z.enum(["mercado_pago", "manual"]).optional(),
      external_id: z.string().optional(),
      pix_copy_paste: z.string().optional(),
      pix_qr_base64: z.string().optional(),
      ticket_url: z.string().optional(),
    })
    .nullable(),
  items: z.array(orderItemSnapshot),
  created_at: z.coerce.date(),
});

const collectionListItem = z.object({
  id: z.uuid(),
  name: z.string(),
  slug: z.string(),
  description: z.string().nullable(),
  product_count: z.number().int().nonnegative(),
});

const collectionDetail = collectionListItem.extend({
  products: z.array(productListItem),
});

export const schema = {
  store: {
    listProducts: {
      input: z
        .object({
          category: z.enum(productCategories).optional(),
          featured: z.boolean().optional(),
          search: z.string().optional(),
          collection_slug: z.string().optional(),
          in_stock_only: z.boolean().optional(),
        })
        .optional(),
      output: z.object({ data: z.array(productListItem) }),
    },
    getProduct: {
      input: z.object({ slug: z.string().min(1) }),
      output: productDetail,
    },
    listCollections: {
      input: z.undefined(),
      output: z.object({ data: z.array(collectionListItem) }),
    },
    getCollection: {
      input: z.object({ slug: z.string().min(1) }),
      output: collectionDetail,
    },
    getShippingQuote: {
      input: z.object({
        state: z.string().length(2),
        zip_code: z.string().min(8).max(9),
        subtotal_cents: z.number().int().nonnegative(),
      }),
      output: z.object({
        shipping_cents: z.number().int().nonnegative(),
        free_shipping: z.boolean(),
        estimated_days: z.string(),
      }),
    },
    createOrder: {
      input: z.object({
        guest_email: z.email(),
        items: z
          .array(
            z.object({
              variant_id: z.uuid(),
              quantity: z.number().int().positive().max(20),
            }),
          )
          .min(1),
        shipping_address: shippingAddressSchema,
        notes: z.string().max(500).optional(),
        coupon_code: z.string().max(32).optional(),
      }),
      output: storeOrder,
    },
    getOrder: {
      input: z.object({ id: z.uuid() }),
      output: storeOrder,
    },
    listMyOrders: {
      input: z.undefined(),
      output: z.object({ data: z.array(orderListItem) }),
    },
  },
  admin: {
    listProducts: {
      input: z.undefined(),
      output: z.object({ data: z.array(adminProduct) }),
    },
    createProduct: {
      input: z.object({
        name: z.string().min(1),
        slug: z.string().min(1),
        description: z.string().optional(),
        price_in_cents: z.number().int().positive(),
        category: z.enum(productCategories).default("biquini"),
        featured: z.boolean().default(false),
        variants: z
          .array(
            z.object({
              sku: z.string().min(1),
              size: z.enum(productSizes),
              color: z.string().min(1),
              quantity: z.number().int().nonnegative().default(0),
              price_in_cents: z.number().int().positive().optional(),
            }),
          )
          .min(1),
      }),
      output: productDetail,
    },
    updateProduct: {
      input: z.object({
        id: z.uuid(),
        name: z.string().min(1),
        slug: z.string().min(1),
        description: z.string().optional(),
        price_in_cents: z.number().int().positive(),
        category: z.enum(productCategories),
        featured: z.boolean(),
        active: z.boolean(),
      }),
      output: productDetail,
    },
    listInventory: {
      input: z
        .object({
          warehouse_id: z.uuid().optional(),
          low_stock_only: z.boolean().optional(),
        })
        .optional(),
      output: z.object({ data: z.array(inventoryItem) }),
    },
    listLowStockAlerts: {
      input: z.undefined(),
      output: z.object({
        threshold: z.number().int(),
        data: z.array(inventoryItem),
      }),
    },
    listInventoryMovements: {
      input: z.object({
        variant_id: z.uuid().optional(),
        warehouse_id: z.uuid().optional(),
        type: z
          .enum([
            "entrada",
            "saida",
            "reserva",
            "liberacao",
            "ajuste",
            "venda",
            "transferencia",
          ])
          .optional(),
        search: z.string().optional(),
        limit: z.number().int().min(1).max(200).optional(),
        offset: z.number().int().min(0).optional(),
      }),
      output: z.object({
        data: z.array(inventoryMovement),
        limit: z.number().int(),
        offset: z.number().int(),
      }),
    },
    receiveInventory: {
      input: z.object({
        variant_id: z.uuid(),
        warehouse_id: z.uuid().optional(),
        quantity: z.number().int().positive(),
        notes: z.string().max(500).optional(),
      }),
      output: inventoryItem,
    },
    transferInventory: {
      input: z.object({
        variant_id: z.uuid(),
        from_warehouse_id: z.uuid(),
        to_warehouse_id: z.uuid(),
        quantity: z.number().int().positive(),
        notes: z.string().max(500).optional(),
      }),
      output: inventoryItem,
    },
    exportInventoryCsv: {
      input: z.undefined(),
      output: z.object({
        filename: z.string(),
        content: z.string(),
      }),
    },
    importInventoryCsv: {
      input: z.object({
        csv: z.string().min(1),
      }),
      output: z.object({
        processed: z.number().int(),
        updated: z.number().int(),
        skipped: z.number().int(),
        errors: z.number().int(),
        results: z.array(
          z.object({
            sku: z.string(),
            status: z.enum(["updated", "skipped", "error"]),
            message: z.string().optional(),
          }),
        ),
      }),
    },
    listWarehouses: {
      input: z.undefined(),
      output: z.object({ data: z.array(warehouseItem) }),
    },
    createWarehouse: {
      input: z.object({
        code: z.string().min(1).max(32),
        name: z.string().min(1).max(120),
      }),
      output: warehouseItem,
    },
    adjustInventory: {
      input: z.object({
        variant_id: z.uuid(),
        warehouse_id: z.uuid().optional(),
        quantity_delta: z.number().int(),
        notes: z.string().optional(),
      }),
      output: inventoryItem,
    },
    listOrders: {
      input: z.undefined(),
      output: z.object({ data: z.array(orderListItem) }),
    },
    getOrder: {
      input: z.object({ id: z.uuid() }),
      output: storeOrder,
    },
    updateOrderStatus: {
      input: z.object({
        id: z.uuid(),
        status: z.enum(orderStatuses),
      }),
      output: storeOrder,
    },
    uploadProductImage: {
      input: z.object({
        product_id: z.uuid(),
        file: z
          .file({ error: "Selecione uma imagem" })
          .mime(["image/jpeg", "image/png", "image/webp"], {
            error: "Apenas imagens JPG, PNG ou WebP são permitidas",
          })
          .max(5 * 1024 * 1024, { error: "A imagem deve ter no máximo 5MB" }),
        alt: z.string().max(120).optional(),
      }),
      output: z.object({
        id: z.uuid(),
        url: z.string(),
        alt: z.string().nullable(),
        sort_order: z.number(),
      }),
    },
    deleteProductImage: {
      input: z.object({ id: z.uuid() }),
      output: z.object({ success: z.literal(true) }),
    },
  },
};

export type ProductListItem = z.infer<typeof productListItem>;
export type ProductDetail = z.infer<typeof productDetail>;
export type AdminProduct = z.infer<typeof adminProduct>;
export type InventoryItem = z.infer<typeof inventoryItem>;
export type OrderListItem = z.infer<typeof orderListItem>;
export type StoreOrder = z.infer<typeof storeOrder>;
export type ShippingAddress = z.infer<typeof shippingAddressSchema>;
