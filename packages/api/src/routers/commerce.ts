import type { Context } from "@lindaflor/api/context";
import { o, publicProcedure } from "@lindaflor/api/middlewares";
import { authorize } from "@lindaflor/api/middlewares/authorize";
import { authorizedProcedure } from "@lindaflor/api/middlewares/authorized";
import {
  getStoreCollectionBySlug,
  listStoreCollections,
} from "@lindaflor/core/commerce/collections";
import {
  createStoreOrder,
  deleteProductImage,
  getStoreOrder,
  uploadProductImage,
} from "@lindaflor/core/commerce/checkout";
import {
  adjustInventory,
  exportInventoryCsv,
  importInventoryCsv,
  listInventory,
  listInventoryMovements,
  listLowStockAlerts,
  receiveInventory,
  transferInventory,
} from "@lindaflor/core/commerce/inventory";
import {
  createWarehouse,
  listWarehouses,
} from "@lindaflor/core/commerce/warehouses";
import {
  getAdminOrder,
  listCustomerOrders,
  listOrders,
  updateOrderStatus,
} from "@lindaflor/core/commerce/orders";
import {
  createProduct,
  getStoreProductBySlug,
  listAdminProducts,
  listStoreProducts,
  updateProduct,
} from "@lindaflor/core/commerce/products";
import {
  calculateShippingCents,
} from "@lindaflor/core/commerce/shipping";
import { schema } from "@lindaflor/shared/schemas/commerce";
import type { EnhancedRouter } from "@orpc/server";

const storeRoutes = {
  listProducts: publicProcedure
    .route({
      method: "GET",
      path: "/products",
      summary: "Listar produtos da loja",
    })
    .input(schema.store.listProducts.input)
    .output(schema.store.listProducts.output)
    .handler(async ({ input }) => listStoreProducts(input)),

  getProduct: publicProcedure
    .route({
      method: "GET",
      path: "/products/{slug}",
      summary: "Detalhe do produto",
    })
    .input(schema.store.getProduct.input)
    .output(schema.store.getProduct.output)
    .handler(async ({ input }) => getStoreProductBySlug(input.slug)),

  listCollections: publicProcedure
    .route({
      method: "GET",
      path: "/collections",
      summary: "Listar coleções",
    })
    .input(schema.store.listCollections.input)
    .output(schema.store.listCollections.output)
    .handler(async () => listStoreCollections()),

  getCollection: publicProcedure
    .route({
      method: "GET",
      path: "/collections/{slug}",
      summary: "Detalhe da coleção",
    })
    .input(schema.store.getCollection.input)
    .output(schema.store.getCollection.output)
    .handler(async ({ input }) => getStoreCollectionBySlug(input.slug)),

  getShippingQuote: publicProcedure
    .route({
      method: "POST",
      path: "/shipping/quote",
      summary: "Cotação de frete",
    })
    .input(schema.store.getShippingQuote.input)
    .output(schema.store.getShippingQuote.output)
    .handler(async ({ input }) => calculateShippingCents(input)),

  createOrder: publicProcedure
    .route({
      method: "POST",
      path: "/orders",
      summary: "Criar pedido",
    })
    .input(schema.store.createOrder.input)
    .output(schema.store.createOrder.output)
    .handler(async ({ input, context }) =>
      createStoreOrder(input, context.session?.user?.id),
    ),

  getOrder: publicProcedure
    .route({
      method: "GET",
      path: "/orders/{id}",
      summary: "Detalhe do pedido",
    })
    .input(schema.store.getOrder.input)
    .output(schema.store.getOrder.output)
    .handler(async ({ input }) => getStoreOrder(input.id)),

  listMyOrders: authorizedProcedure
    .route({
      method: "GET",
      path: "/orders/me",
      summary: "Meus pedidos",
    })
    .input(schema.store.listMyOrders.input)
    .output(schema.store.listMyOrders.output)
    .handler(async ({ context }) =>
      listCustomerOrders({
        userId: context.session.user.id,
        email: context.session.user.email,
      }),
    ),
};

const adminRoutes = {
  listProducts: authorizedProcedure
    .use(authorize("read", "Product"))
    .route({
      method: "GET",
      path: "/products",
      summary: "Listar produtos (admin)",
    })
    .input(schema.admin.listProducts.input)
    .output(schema.admin.listProducts.output)
    .handler(async () => listAdminProducts()),

  createProduct: authorizedProcedure
    .use(authorize("create", "Product"))
    .route({
      method: "POST",
      path: "/products",
      summary: "Criar produto",
    })
    .input(schema.admin.createProduct.input)
    .output(schema.admin.createProduct.output)
    .handler(async ({ input }) => createProduct(input)),

  updateProduct: authorizedProcedure
    .use(authorize("update", "Product"))
    .route({
      method: "PATCH",
      path: "/products/{id}",
      summary: "Atualizar produto",
    })
    .input(schema.admin.updateProduct.input)
    .output(schema.admin.updateProduct.output)
    .handler(async ({ input }) => updateProduct(input)),

  listInventory: authorizedProcedure
    .use(authorize("read", "Inventory"))
    .route({
      method: "GET",
      path: "/inventory",
      summary: "Listar estoque",
    })
    .input(schema.admin.listInventory.input)
    .output(schema.admin.listInventory.output)
    .handler(async ({ input }) => listInventory(input)),

  listLowStockAlerts: authorizedProcedure
    .use(authorize("read", "Inventory"))
    .route({
      method: "GET",
      path: "/inventory/low-stock",
      summary: "Alertas de estoque baixo",
    })
    .input(schema.admin.listLowStockAlerts.input)
    .output(schema.admin.listLowStockAlerts.output)
    .handler(async () => listLowStockAlerts()),

  listInventoryMovements: authorizedProcedure
    .use(authorize("read", "Inventory"))
    .route({
      method: "GET",
      path: "/inventory/movements",
      summary: "Histórico de movimentações",
    })
    .input(schema.admin.listInventoryMovements.input)
    .output(schema.admin.listInventoryMovements.output)
    .handler(async ({ input }) => listInventoryMovements(input)),

  receiveInventory: authorizedProcedure
    .use(authorize("update", "Inventory"))
    .route({
      method: "POST",
      path: "/inventory/receive",
      summary: "Entrada de mercadoria",
    })
    .input(schema.admin.receiveInventory.input)
    .output(schema.admin.receiveInventory.output)
    .handler(async ({ input, context }) =>
      receiveInventory(input, context.session.user.id),
    ),

  transferInventory: authorizedProcedure
    .use(authorize("update", "Inventory"))
    .route({
      method: "POST",
      path: "/inventory/transfer",
      summary: "Transferir entre depósitos",
    })
    .input(schema.admin.transferInventory.input)
    .output(schema.admin.transferInventory.output)
    .handler(async ({ input, context }) =>
      transferInventory(input, context.session.user.id),
    ),

  exportInventoryCsv: authorizedProcedure
    .use(authorize("read", "Inventory"))
    .route({
      method: "GET",
      path: "/inventory/export",
      summary: "Exportar estoque CSV",
    })
    .input(schema.admin.exportInventoryCsv.input)
    .output(schema.admin.exportInventoryCsv.output)
    .handler(async () => exportInventoryCsv()),

  importInventoryCsv: authorizedProcedure
    .use(authorize("update", "Inventory"))
    .route({
      method: "POST",
      path: "/inventory/import",
      summary: "Importar estoque CSV",
    })
    .input(schema.admin.importInventoryCsv.input)
    .output(schema.admin.importInventoryCsv.output)
    .handler(async ({ input, context }) =>
      importInventoryCsv(input, context.session.user.id),
    ),

  listWarehouses: authorizedProcedure
    .use(authorize("read", "Inventory"))
    .route({
      method: "GET",
      path: "/warehouses",
      summary: "Listar depósitos",
    })
    .input(schema.admin.listWarehouses.input)
    .output(schema.admin.listWarehouses.output)
    .handler(async () => listWarehouses()),

  createWarehouse: authorizedProcedure
    .use(authorize("update", "Inventory"))
    .route({
      method: "POST",
      path: "/warehouses",
      summary: "Criar depósito",
    })
    .input(schema.admin.createWarehouse.input)
    .output(schema.admin.createWarehouse.output)
    .handler(async ({ input }) => createWarehouse(input)),

  adjustInventory: authorizedProcedure
    .use(authorize("update", "Inventory"))
    .route({
      method: "POST",
      path: "/inventory/adjust",
      summary: "Ajustar estoque",
    })
    .input(schema.admin.adjustInventory.input)
    .output(schema.admin.adjustInventory.output)
    .handler(async ({ input, context }) =>
      adjustInventory(input, context.session.user.id),
    ),

  listOrders: authorizedProcedure
    .use(authorize("read", "Order"))
    .route({
      method: "GET",
      path: "/orders",
      summary: "Listar pedidos",
    })
    .input(schema.admin.listOrders.input)
    .output(schema.admin.listOrders.output)
    .handler(async () => listOrders()),

  getOrder: authorizedProcedure
    .use(authorize("read", "Order"))
    .route({
      method: "GET",
      path: "/orders/{id}",
      summary: "Detalhe do pedido (admin)",
    })
    .input(schema.admin.getOrder.input)
    .output(schema.admin.getOrder.output)
    .handler(async ({ input }) => getAdminOrder(input.id)),

  updateOrderStatus: authorizedProcedure
    .use(authorize("update", "Order"))
    .route({
      method: "PATCH",
      path: "/orders/{id}/status",
      summary: "Atualizar status do pedido",
    })
    .input(schema.admin.updateOrderStatus.input)
    .output(schema.admin.updateOrderStatus.output)
    .handler(async ({ input }) => updateOrderStatus(input)),

  uploadProductImage: authorizedProcedure
    .use(authorize("update", "Product"))
    .route({
      method: "POST",
      path: "/products/images",
      summary: "Upload de imagem de produto",
    })
    .input(schema.admin.uploadProductImage.input)
    .output(schema.admin.uploadProductImage.output)
    .handler(async ({ input }) => uploadProductImage(input)),

  deleteProductImage: authorizedProcedure
    .use(authorize("update", "Product"))
    .route({
      method: "DELETE",
      path: "/products/images/{id}",
      summary: "Remover imagem de produto",
    })
    .input(schema.admin.deleteProductImage.input)
    .output(schema.admin.deleteProductImage.output)
    .handler(async ({ input }) => deleteProductImage(input)),
};

type StoreRoutes = typeof storeRoutes;
type AdminRoutes = typeof adminRoutes;

type StoreRouter = EnhancedRouter<
  StoreRoutes,
  Context,
  Context,
  Record<never, never>
>;

type AdminRouter = EnhancedRouter<
  AdminRoutes,
  Context,
  Context,
  Record<never, never>
>;

type CommerceRouter = {
  store: StoreRouter;
  admin: AdminRouter;
};

export const commerceRouter: CommerceRouter = {
  store: o.prefix("/store").tag("Store").router(storeRoutes),
  admin: o.prefix("/admin").tag("Admin").router(adminRoutes),
};
