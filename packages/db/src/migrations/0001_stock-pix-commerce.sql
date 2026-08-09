CREATE TYPE "public"."pix_key_type" AS ENUM('cpf', 'cnpj', 'email', 'phone', 'random');--> statement-breakpoint
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
--> statement-breakpoint
ALTER TABLE "order_items" ADD COLUMN "warehouse_id" uuid;--> statement-breakpoint
ALTER TABLE "order_items" ADD CONSTRAINT "order_items_warehouse_id_warehouses_id_fk" FOREIGN KEY ("warehouse_id") REFERENCES "public"."warehouses"("id") ON DELETE restrict ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "order_items_warehouse_id_idx" ON "order_items" USING btree ("warehouse_id");--> statement-breakpoint
UPDATE "order_items"
SET "warehouse_id" = (
  SELECT "id" FROM "warehouses" WHERE "is_default" = true ORDER BY "created_at" ASC LIMIT 1
)
WHERE "warehouse_id" IS NULL;