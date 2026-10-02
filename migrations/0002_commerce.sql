DROP TABLE IF EXISTS order_items, orders, inventory, product_variants, product_images, products, collections, warehouses, store_settings CASCADE;
DROP TYPE IF EXISTS "public"."order_status" CASCADE;
DROP TYPE IF EXISTS "public"."product_category" CASCADE;
DROP TYPE IF EXISTS "public"."product_size" CASCADE;
DROP TYPE IF EXISTS "public"."pix_key_type" CASCADE;

CREATE TYPE "public"."order_status" AS ENUM('pending_payment', 'paid', 'processing', 'shipped', 'delivered', 'cancelled');
CREATE TYPE "public"."product_category" AS ENUM('biquini', 'maio', 'saida_praia', 'acessorio');
CREATE TYPE "public"."product_size" AS ENUM('pp', 'p', 'm', 'g', 'gg');
CREATE TYPE "public"."pix_key_type" AS ENUM('cpf', 'cnpj', 'email', 'phone', 'random');

CREATE TABLE "collections" (
    "id" uuid PRIMARY KEY NOT NULL,
    "name" text NOT NULL,
    "slug" text NOT NULL,
    "description" text,
    "active" boolean DEFAULT true NOT NULL,
    "created_at" timestamp DEFAULT now() NOT NULL,
    "updated_at" timestamp DEFAULT now() NOT NULL
);

CREATE TABLE "products" (
    "id" uuid PRIMARY KEY NOT NULL,
    "name" text NOT NULL,
    "slug" text NOT NULL,
    "description" text,
    "price_in_cents" integer NOT NULL,
    "category" "product_category" DEFAULT 'biquini' NOT NULL,
    "collection_id" uuid,
    "active" boolean DEFAULT true NOT NULL,
    "featured" boolean DEFAULT false NOT NULL,
    "created_at" timestamp DEFAULT now() NOT NULL,
    "updated_at" timestamp DEFAULT now() NOT NULL
);

CREATE TABLE "product_images" (
    "id" uuid PRIMARY KEY NOT NULL,
    "product_id" uuid NOT NULL,
    "url" text NOT NULL,
    "alt" text,
    "sort_order" integer DEFAULT 0 NOT NULL,
    "created_at" timestamp DEFAULT now() NOT NULL
);

CREATE TABLE "product_variants" (
    "id" uuid PRIMARY KEY NOT NULL,
    "product_id" uuid NOT NULL,
    "sku" text NOT NULL,
    "size" "product_size" NOT NULL,
    "color" text NOT NULL,
    "price_in_cents" integer,
    "low_stock_threshold" integer DEFAULT 5 NOT NULL,
    "created_at" timestamp DEFAULT now() NOT NULL,
    "updated_at" timestamp DEFAULT now() NOT NULL
);

CREATE TABLE "warehouses" (
    "id" uuid PRIMARY KEY NOT NULL,
    "code" text NOT NULL,
    "name" text NOT NULL,
    "is_default" boolean DEFAULT false NOT NULL,
    "active" boolean DEFAULT true NOT NULL,
    "created_at" timestamp DEFAULT now() NOT NULL
);

CREATE TABLE "inventory" (
    "id" uuid PRIMARY KEY NOT NULL,
    "variant_id" uuid NOT NULL,
    "warehouse_id" uuid NOT NULL,
    "quantity" integer DEFAULT 0 NOT NULL,
    "reserved" integer DEFAULT 0 NOT NULL,
    "updated_at" timestamp DEFAULT now() NOT NULL
);

CREATE TABLE "orders" (
    "id" uuid PRIMARY KEY NOT NULL,
    "user_id" uuid,
    "guest_email" text,
    "status" "order_status" DEFAULT 'pending_payment' NOT NULL,
    "subtotal_cents" integer DEFAULT 0 NOT NULL,
    "shipping_cents" integer DEFAULT 0 NOT NULL,
    "discount_cents" integer DEFAULT 0 NOT NULL,
    "total_cents" integer DEFAULT 0 NOT NULL,
    "shipping_address" jsonb,
    "notes" text,
    "payment_meta" jsonb,
    "created_at" timestamp DEFAULT now() NOT NULL,
    "updated_at" timestamp DEFAULT now() NOT NULL
);

CREATE TABLE "order_items" (
    "id" uuid PRIMARY KEY NOT NULL,
    "order_id" uuid NOT NULL,
    "variant_id" uuid NOT NULL,
    "product_name" text NOT NULL,
    "variant_label" text NOT NULL,
    "quantity" integer NOT NULL,
    "unit_price_cents" integer NOT NULL,
    "created_at" timestamp DEFAULT now() NOT NULL
);

CREATE TABLE "store_settings" (
    "id" uuid PRIMARY KEY NOT NULL,
    "pix_key" text,
    "pix_key_type" "pix_key_type",
    "pix_merchant_name" text,
    "pix_merchant_city" text,
    "whatsapp_number" text,
    "whatsapp_message_template" text,
    "created_at" timestamp DEFAULT now() NOT NULL,
    "updated_at" timestamp DEFAULT now() NOT NULL
);

ALTER TABLE "products" ADD CONSTRAINT "products_collection_id_collections_id_fk" FOREIGN KEY ("collection_id") REFERENCES "public"."collections"("id") ON DELETE set null ON UPDATE no action;
ALTER TABLE "product_images" ADD CONSTRAINT "product_images_product_id_products_id_fk" FOREIGN KEY ("product_id") REFERENCES "public"."products"("id") ON DELETE cascade ON UPDATE no action;
ALTER TABLE "product_variants" ADD CONSTRAINT "product_variants_product_id_products_id_fk" FOREIGN KEY ("product_id") REFERENCES "public"."products"("id") ON DELETE cascade ON UPDATE no action;
ALTER TABLE "inventory" ADD CONSTRAINT "inventory_variant_id_product_variants_id_fk" FOREIGN KEY ("variant_id") REFERENCES "public"."product_variants"("id") ON DELETE cascade ON UPDATE no action;
ALTER TABLE "inventory" ADD CONSTRAINT "inventory_warehouse_id_warehouses_id_fk" FOREIGN KEY ("warehouse_id") REFERENCES "public"."warehouses"("id") ON DELETE restrict ON UPDATE no action;
ALTER TABLE "order_items" ADD CONSTRAINT "order_items_order_id_orders_id_fk" FOREIGN KEY ("order_id") REFERENCES "public"."orders"("id") ON DELETE cascade ON UPDATE no action;
ALTER TABLE "order_items" ADD CONSTRAINT "order_items_variant_id_product_variants_id_fk" FOREIGN KEY ("variant_id") REFERENCES "public"."product_variants"("id") ON DELETE restrict ON UPDATE no action;

CREATE UNIQUE INDEX "collections_slug_uidx" ON "collections" USING btree ("slug");
CREATE UNIQUE INDEX "products_slug_uidx" ON "products" USING btree ("slug");
CREATE INDEX "products_collection_id_idx" ON "products" USING btree ("collection_id");
CREATE INDEX "products_active_idx" ON "products" USING btree ("active");
CREATE INDEX "product_images_product_id_idx" ON "product_images" USING btree ("product_id");
CREATE UNIQUE INDEX "product_variants_sku_uidx" ON "product_variants" USING btree ("sku");
CREATE INDEX "product_variants_product_id_idx" ON "product_variants" USING btree ("product_id");
CREATE UNIQUE INDEX "inventory_variant_warehouse_uidx" ON "inventory" USING btree ("variant_id", "warehouse_id");
CREATE INDEX "order_items_order_id_idx" ON "order_items" USING btree ("order_id");
CREATE INDEX "orders_status_idx" ON "orders" USING btree ("status");
CREATE UNIQUE INDEX "warehouses_code_uidx" ON "warehouses" USING btree ("code");
