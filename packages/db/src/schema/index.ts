import {
  accounts,
  invitations,
  jwkss,
  members,
  organizations,
  sessions,
  two_factors,
  users,
  verifications,
} from "@lindaflor/db/schema/auth";
import {
  collections,
  inventory,
  inventory_movements,
  order_items,
  orders,
  product_images,
  product_variants,
  products,
  warehouses,
} from "@lindaflor/db/schema/commerce";

export const schema = {
  users,
  accounts,
  sessions,
  verifications,
  organizations,
  members,
  invitations,
  two_factors,
  jwkss,
  collections,
  products,
  product_images,
  product_variants,
  inventory,
  inventory_movements,
  orders,
  order_items,
  warehouses,
};
export type DBSchema = typeof schema;
