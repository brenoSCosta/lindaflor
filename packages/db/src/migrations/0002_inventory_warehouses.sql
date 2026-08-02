CREATE TABLE IF NOT EXISTS "warehouses" (
  "id" uuid PRIMARY KEY NOT NULL,
  "code" text NOT NULL,
  "name" text NOT NULL,
  "is_default" boolean DEFAULT false NOT NULL,
  "active" boolean DEFAULT true NOT NULL,
  "created_at" timestamp DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE UNIQUE INDEX IF NOT EXISTS "warehouses_code_uidx" ON "warehouses" ("code");
--> statement-breakpoint
CREATE INDEX IF NOT EXISTS "warehouses_is_default_idx" ON "warehouses" ("is_default");
--> statement-breakpoint
INSERT INTO "warehouses" ("id", "code", "name", "is_default", "active")
VALUES ('0194f0a0-0000-7000-8000-000000000001', 'principal', 'Loja Principal', true, true)
ON CONFLICT DO NOTHING;
--> statement-breakpoint
ALTER TABLE "product_variants" ADD COLUMN IF NOT EXISTS "low_stock_threshold" integer DEFAULT 5 NOT NULL;
--> statement-breakpoint
ALTER TABLE "inventory" ADD COLUMN IF NOT EXISTS "warehouse_id" uuid;
--> statement-breakpoint
UPDATE "inventory"
SET "warehouse_id" = '0194f0a0-0000-7000-8000-000000000001'
WHERE "warehouse_id" IS NULL;
--> statement-breakpoint
ALTER TABLE "inventory" ALTER COLUMN "warehouse_id" SET NOT NULL;
--> statement-breakpoint
ALTER TABLE "inventory" ADD CONSTRAINT "inventory_warehouse_id_warehouses_id_fk"
  FOREIGN KEY ("warehouse_id") REFERENCES "warehouses"("id") ON DELETE restrict ON UPDATE no action;
--> statement-breakpoint
DROP INDEX IF EXISTS "inventory_variant_id_uidx";
--> statement-breakpoint
CREATE UNIQUE INDEX IF NOT EXISTS "inventory_variant_warehouse_uidx"
  ON "inventory" ("variant_id", "warehouse_id");
--> statement-breakpoint
CREATE INDEX IF NOT EXISTS "inventory_warehouse_id_idx" ON "inventory" ("warehouse_id");
--> statement-breakpoint
ALTER TABLE "inventory_movements" ADD COLUMN IF NOT EXISTS "warehouse_id" uuid;
--> statement-breakpoint
UPDATE "inventory_movements" im
SET "warehouse_id" = i."warehouse_id"
FROM "inventory" i
WHERE i."variant_id" = im."variant_id"
  AND im."warehouse_id" IS NULL;
--> statement-breakpoint
UPDATE "inventory_movements"
SET "warehouse_id" = '0194f0a0-0000-7000-8000-000000000001'
WHERE "warehouse_id" IS NULL;
--> statement-breakpoint
ALTER TABLE "inventory_movements" ADD CONSTRAINT "inventory_movements_warehouse_id_warehouses_id_fk"
  FOREIGN KEY ("warehouse_id") REFERENCES "warehouses"("id") ON DELETE set null ON UPDATE no action;
--> statement-breakpoint
ALTER TYPE "inventory_movement_type" ADD VALUE IF NOT EXISTS 'transferencia';
