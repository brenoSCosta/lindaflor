CREATE TYPE "public"."inventory_movement_type" AS ENUM('entrada', 'saida', 'reserva', 'liberacao', 'ajuste', 'venda');--> statement-breakpoint
CREATE TYPE "public"."order_status" AS ENUM('pending_payment', 'paid', 'processing', 'shipped', 'delivered', 'cancelled');--> statement-breakpoint
CREATE TYPE "public"."product_category" AS ENUM('biquini', 'maio', 'saida_praia', 'acessorio');--> statement-breakpoint
CREATE TYPE "public"."product_size" AS ENUM('pp', 'p', 'm', 'g', 'gg');--> statement-breakpoint
CREATE TYPE "public"."powersync_upload_action" AS ENUM('PUT', 'PATCH', 'DELETE');--> statement-breakpoint
CREATE TYPE "public"."powersync_upload_status" AS ENUM('success', 'failed', 'forbidden', 'invalid', 'unsupported');--> statement-breakpoint
CREATE TYPE "public"."lab_oil_sample_type" AS ENUM('top', 'middle', 'bottom', 'inline', 'running', 'residual');--> statement-breakpoint
CREATE TYPE "public"."measurement_equipment_type" AS ENUM('manual', 'electronic');--> statement-breakpoint
CREATE TYPE "public"."tank_day_bulletin_status" AS ENUM('open', 'approved');--> statement-breakpoint
CREATE TYPE "public"."label" AS ENUM('bug', 'feature', 'documentation');--> statement-breakpoint
CREATE TYPE "public"."priority" AS ENUM('low', 'medium', 'high');--> statement-breakpoint
CREATE TYPE "public"."status" AS ENUM('backlog', 'todo', 'in progress', 'done', 'canceled');--> statement-breakpoint
CREATE TABLE "audit_events" (
	"id" uuid PRIMARY KEY NOT NULL,
	"organization_id" uuid NOT NULL,
	"aggregate_type" text NOT NULL,
	"aggregate_id" text NOT NULL,
	"entity_type" text NOT NULL,
	"entity_id" uuid,
	"action" text NOT NULL,
	"actor_user_id" uuid,
	"actor_name" text NOT NULL,
	"metadata" jsonb,
	"occurred_at" timestamp with time zone NOT NULL,
	"created_at" timestamp DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE TABLE "accounts" (
	"id" uuid PRIMARY KEY NOT NULL,
	"account_id" text NOT NULL,
	"provider_id" text NOT NULL,
	"user_id" uuid NOT NULL,
	"access_token" text,
	"refresh_token" text,
	"id_token" text,
	"access_token_expires_at" timestamp,
	"refresh_token_expires_at" timestamp,
	"scope" text,
	"password" text,
	"created_at" timestamp DEFAULT now() NOT NULL,
	"updated_at" timestamp NOT NULL
);
--> statement-breakpoint
CREATE TABLE "invitations" (
	"id" uuid PRIMARY KEY NOT NULL,
	"organization_id" uuid NOT NULL,
	"email" text NOT NULL,
	"role" text,
	"status" text DEFAULT 'pending' NOT NULL,
	"expires_at" timestamp NOT NULL,
	"created_at" timestamp DEFAULT now() NOT NULL,
	"inviter_id" uuid NOT NULL
);
--> statement-breakpoint
CREATE TABLE "jwkss" (
	"id" text PRIMARY KEY NOT NULL,
	"public_key" text NOT NULL,
	"private_key" text NOT NULL,
	"created_at" timestamp NOT NULL,
	"expires_at" timestamp
);
--> statement-breakpoint
CREATE TABLE "members" (
	"id" uuid PRIMARY KEY NOT NULL,
	"organization_id" uuid NOT NULL,
	"user_id" uuid NOT NULL,
	"role" text DEFAULT 'member' NOT NULL,
	"created_at" timestamp NOT NULL
);
--> statement-breakpoint
CREATE TABLE "organizations" (
	"id" uuid PRIMARY KEY NOT NULL,
	"name" text NOT NULL,
	"slug" text NOT NULL,
	"logo" text,
	"created_at" timestamp NOT NULL,
	"metadata" text,
	CONSTRAINT "organizations_slug_unique" UNIQUE("slug")
);
--> statement-breakpoint
CREATE TABLE "sessions" (
	"id" uuid PRIMARY KEY NOT NULL,
	"expires_at" timestamp NOT NULL,
	"token" text NOT NULL,
	"created_at" timestamp DEFAULT now() NOT NULL,
	"updated_at" timestamp NOT NULL,
	"ip_address" text,
	"user_agent" text,
	"user_id" uuid NOT NULL,
	"impersonated_by" uuid,
	"active_organization_id" uuid,
	CONSTRAINT "sessions_token_unique" UNIQUE("token")
);
--> statement-breakpoint
CREATE TABLE "two_factor" (
	"id" uuid PRIMARY KEY NOT NULL,
	"secret" text NOT NULL,
	"backup_codes" text NOT NULL,
	"user_id" uuid NOT NULL,
	"verified" boolean DEFAULT false NOT NULL
);
--> statement-breakpoint
CREATE TABLE "users" (
	"id" uuid PRIMARY KEY NOT NULL,
	"name" text NOT NULL,
	"email" text NOT NULL,
	"email_verified" boolean DEFAULT false NOT NULL,
	"image" text,
	"created_at" timestamp DEFAULT now() NOT NULL,
	"updated_at" timestamp DEFAULT now() NOT NULL,
	"two_factor_enabled" boolean DEFAULT false,
	"role" text,
	"banned" boolean DEFAULT false,
	"ban_reason" text,
	"ban_expires" timestamp,
	CONSTRAINT "users_email_unique" UNIQUE("email")
);
--> statement-breakpoint
CREATE TABLE "verifications" (
	"id" uuid PRIMARY KEY NOT NULL,
	"identifier" text NOT NULL,
	"value" text NOT NULL,
	"expires_at" timestamp NOT NULL,
	"created_at" timestamp DEFAULT now() NOT NULL,
	"updated_at" timestamp DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE TABLE "career" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"title" text NOT NULL,
	"department" text NOT NULL,
	"location" text NOT NULL,
	"type" text NOT NULL,
	"description" text,
	"requirements" text[] DEFAULT '{}' NOT NULL,
	"is_active" boolean DEFAULT true NOT NULL,
	"created_at" timestamp DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE TABLE "collections" (
	"id" uuid PRIMARY KEY NOT NULL,
	"name" text NOT NULL,
	"slug" text NOT NULL,
	"description" text,
	"active" boolean DEFAULT true NOT NULL,
	"created_at" timestamp DEFAULT now() NOT NULL,
	"updated_at" timestamp DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE TABLE "inventory" (
	"id" uuid PRIMARY KEY NOT NULL,
	"variant_id" uuid NOT NULL,
	"quantity" integer DEFAULT 0 NOT NULL,
	"reserved" integer DEFAULT 0 NOT NULL,
	"updated_at" timestamp DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE TABLE "inventory_movements" (
	"id" uuid PRIMARY KEY NOT NULL,
	"variant_id" uuid NOT NULL,
	"type" "inventory_movement_type" NOT NULL,
	"quantity" integer NOT NULL,
	"reference_type" text,
	"reference_id" uuid,
	"notes" text,
	"created_by" uuid,
	"created_at" timestamp DEFAULT now() NOT NULL
);
--> statement-breakpoint
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
--> statement-breakpoint
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
	"created_at" timestamp DEFAULT now() NOT NULL,
	"updated_at" timestamp DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE TABLE "product_images" (
	"id" uuid PRIMARY KEY NOT NULL,
	"product_id" uuid NOT NULL,
	"url" text NOT NULL,
	"alt" text,
	"sort_order" integer DEFAULT 0 NOT NULL,
	"created_at" timestamp DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE TABLE "product_variants" (
	"id" uuid PRIMARY KEY NOT NULL,
	"product_id" uuid NOT NULL,
	"sku" text NOT NULL,
	"size" "product_size" NOT NULL,
	"color" text NOT NULL,
	"price_in_cents" integer,
	"created_at" timestamp DEFAULT now() NOT NULL,
	"updated_at" timestamp DEFAULT now() NOT NULL
);
--> statement-breakpoint
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
--> statement-breakpoint
CREATE TABLE "curriculum" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"name" text NOT NULL,
	"email" text NOT NULL,
	"phone" text,
	"headline" text NOT NULL,
	"summary" text,
	"skills" text[] DEFAULT '{}' NOT NULL,
	"career_id" uuid,
	"file_key" text NOT NULL,
	"file_name" text NOT NULL,
	"file_size" integer NOT NULL,
	"mime_type" text DEFAULT 'application/pdf' NOT NULL,
	"submitted_at" timestamp DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE TABLE "powersync_upload_operations" (
	"id" uuid PRIMARY KEY NOT NULL,
	"entity" text NOT NULL,
	"action" "powersync_upload_action" NOT NULL,
	"status" "powersync_upload_status" NOT NULL,
	"client_payload" jsonb NOT NULL,
	"server_payload" jsonb NOT NULL,
	"retries_count" integer DEFAULT 1 NOT NULL,
	"organization_id" uuid NOT NULL,
	"created_by_user_id" uuid NOT NULL,
	"created_at" timestamp DEFAULT now() NOT NULL,
	"updated_at" timestamp DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE TABLE "concessions" (
	"id" uuid PRIMARY KEY NOT NULL,
	"name" text NOT NULL,
	"state" text NOT NULL,
	"organization_id" uuid NOT NULL,
	"created_by_user_id" uuid NOT NULL,
	"created_at" timestamp DEFAULT now() NOT NULL,
	"updated_at" timestamp DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE TABLE "installations" (
	"id" uuid PRIMARY KEY NOT NULL,
	"name" text NOT NULL,
	"concession_id" uuid NOT NULL,
	"organization_id" uuid NOT NULL,
	"created_by_user_id" uuid NOT NULL,
	"created_at" timestamp DEFAULT now() NOT NULL,
	"updated_at" timestamp DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE TABLE "lab_oil_analyses" (
	"id" uuid PRIMARY KEY NOT NULL,
	"tank_id" uuid NOT NULL,
	"sample_type" "lab_oil_sample_type" NOT NULL,
	"collected_at" timestamp with time zone NOT NULL,
	"issued_at" date NOT NULL,
	"certificate_number" text NOT NULL,
	"laboratory_name" text NOT NULL,
	"method_density" text,
	"method_basic_sediment_water" text,
	"density_at_20c" double precision NOT NULL,
	"water_and_sediment_percent" double precision NOT NULL,
	"salinity" double precision,
	"organization_id" uuid NOT NULL,
	"created_by_user_id" uuid NOT NULL,
	"created_at" timestamp DEFAULT now() NOT NULL,
	"updated_at" timestamp DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE TABLE "measurement_equipments" (
	"id" uuid PRIMARY KEY NOT NULL,
	"code" text NOT NULL,
	"description" text,
	"type" "measurement_equipment_type" DEFAULT 'manual' NOT NULL,
	"length_m" double precision,
	"reference_height_m" double precision,
	"manufacturer" text,
	"serial_number" text,
	"calibrated_at" date,
	"calibration_valid_until" date,
	"active" boolean DEFAULT true NOT NULL,
	"organization_id" uuid NOT NULL,
	"created_by_user_id" uuid NOT NULL,
	"created_at" timestamp DEFAULT now() NOT NULL,
	"updated_at" timestamp DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE TABLE "tank_calibration_points" (
	"id" uuid PRIMARY KEY NOT NULL,
	"calibration_id" uuid NOT NULL,
	"height_cm" double precision NOT NULL,
	"volume_m3" double precision NOT NULL
);
--> statement-breakpoint
CREATE TABLE "tank_calibrations" (
	"id" uuid PRIMARY KEY NOT NULL,
	"tank_id" uuid NOT NULL,
	"certificate_number" text NOT NULL,
	"issued_at" date,
	"valid_from" date NOT NULL,
	"valid_until" date,
	"organization_id" uuid NOT NULL,
	"created_by_user_id" uuid NOT NULL,
	"created_at" timestamp DEFAULT now() NOT NULL,
	"updated_at" timestamp DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE TABLE "tank_day_bulletins" (
	"id" uuid PRIMARY KEY NOT NULL,
	"tank_id" uuid NOT NULL,
	"operational_day" date NOT NULL,
	"status" "tank_day_bulletin_status" DEFAULT 'open' NOT NULL,
	"approved_at" timestamp,
	"approved_by_user_id" uuid,
	"reopened_at" timestamp,
	"reopened_by_user_id" uuid,
	"organization_id" uuid NOT NULL,
	"created_at" timestamp DEFAULT now() NOT NULL,
	"updated_at" timestamp DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE TABLE "tank_transfers" (
	"id" uuid PRIMARY KEY NOT NULL,
	"tank_id" uuid NOT NULL,
	"organization_id" uuid NOT NULL,
	"operational_day" date NOT NULL,
	"transferred_at" timestamp with time zone NOT NULL,
	"height_before_m" double precision NOT NULL,
	"height_after_m" double precision NOT NULL,
	"oil_temperature_c" double precision NOT NULL,
	"ambient_temperature_c" double precision NOT NULL,
	"gross_volume_before_m3" double precision NOT NULL,
	"gross_volume_after_m3" double precision NOT NULL,
	"gross_volume_out_m3" double precision NOT NULL,
	"gross_volume_out_m3_20c" double precision,
	"net_oil_volume_out_m3_20c" double precision,
	"shell_temperature_c" double precision,
	"shell_correction_factor" double precision,
	"liquid_correction_factor" double precision,
	"combined_correction_factor" double precision,
	"tank_calibration_id" uuid,
	"lab_oil_analysis_id" uuid,
	"density_at_20c_kg_m3" double precision,
	"water_and_sediment_percent" double precision,
	"destination_label" text,
	"observation" text NOT NULL,
	"tankage_id" uuid NOT NULL,
	"created_by_user_id" uuid NOT NULL,
	"created_at" timestamp DEFAULT now() NOT NULL,
	"updated_at" timestamp DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE TABLE "tankages" (
	"id" uuid PRIMARY KEY NOT NULL,
	"tank_id" uuid NOT NULL,
	"concession_id" uuid NOT NULL,
	"installation_id" uuid NOT NULL,
	"measurement_equipment_id" uuid,
	"operator_user_id" uuid NOT NULL,
	"measured_at" timestamp with time zone NOT NULL,
	"operational_day" date NOT NULL,
	"previous_measurement" double precision NOT NULL,
	"current_measurement" double precision NOT NULL,
	"oil_temperature_c" double precision NOT NULL,
	"ambient_temperature_c" double precision NOT NULL,
	"observation" text NOT NULL,
	"latitude" double precision,
	"longitude" double precision,
	"gross_volume_m3" double precision,
	"gross_volume_m3_20c" double precision,
	"net_oil_volume_m3_20c" double precision,
	"volume_oil_barrels" double precision,
	"shell_temperature_c" double precision,
	"shell_correction_factor" double precision,
	"liquid_correction_factor" double precision,
	"combined_correction_factor" double precision,
	"tank_calibration_id" uuid,
	"lab_oil_analysis_id" uuid,
	"density_at_20c_kg_m3" double precision,
	"water_and_sediment_percent" double precision,
	"organization_id" uuid NOT NULL,
	"created_by_user_id" uuid NOT NULL,
	"created_at" timestamp DEFAULT now() NOT NULL,
	"updated_at" timestamp DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE TABLE "tanks" (
	"id" uuid PRIMARY KEY NOT NULL,
	"tag" text NOT NULL,
	"concession_id" uuid NOT NULL,
	"installation_id" uuid NOT NULL,
	"measurement_equipment_id" uuid,
	"latitude" double precision,
	"longitude" double precision,
	"organization_id" uuid NOT NULL,
	"created_by_user_id" uuid NOT NULL,
	"created_at" timestamp DEFAULT now() NOT NULL,
	"updated_at" timestamp DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE TABLE "todo" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"text" text NOT NULL,
	"status" "status" DEFAULT 'todo' NOT NULL,
	"label" "label" DEFAULT 'documentation' NOT NULL,
	"priority" "priority" DEFAULT 'medium' NOT NULL,
	"estimated_hours" integer DEFAULT 0,
	"actual_hours" integer DEFAULT 0,
	"progress" integer DEFAULT 0,
	"cost" integer DEFAULT 0,
	"due_date" timestamp,
	"completed_at" timestamp,
	"created_at" timestamp DEFAULT now() NOT NULL,
	"updated_at" timestamp DEFAULT now() NOT NULL,
	"organization_id" uuid NOT NULL
);
--> statement-breakpoint
CREATE TABLE "training_courses" (
	"id" uuid PRIMARY KEY NOT NULL,
	"title" text NOT NULL,
	"description" text,
	"is_published" boolean DEFAULT false NOT NULL,
	"organization_id" uuid NOT NULL,
	"created_by_user_id" uuid NOT NULL,
	"created_at" timestamp DEFAULT now() NOT NULL,
	"updated_at" timestamp DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE TABLE "training_enrollments" (
	"id" uuid PRIMARY KEY NOT NULL,
	"course_id" uuid NOT NULL,
	"user_id" uuid NOT NULL,
	"organization_id" uuid NOT NULL,
	"enrolled_at" timestamp DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE TABLE "training_lecture_progress" (
	"id" uuid PRIMARY KEY NOT NULL,
	"lecture_id" uuid NOT NULL,
	"user_id" uuid NOT NULL,
	"organization_id" uuid NOT NULL,
	"status" text DEFAULT 'not_started' NOT NULL,
	"completed_at" timestamp,
	"updated_at" timestamp DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE TABLE "training_lectures" (
	"id" uuid PRIMARY KEY NOT NULL,
	"module_id" uuid NOT NULL,
	"organization_id" uuid NOT NULL,
	"title" text NOT NULL,
	"type" text NOT NULL,
	"youtube_url" text,
	"file_key" text,
	"file_name" text,
	"file_size" integer,
	"mime_type" text,
	"url" text,
	"sort_order" integer DEFAULT 0 NOT NULL,
	"created_at" timestamp DEFAULT now() NOT NULL,
	"updated_at" timestamp DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE TABLE "training_modules" (
	"id" uuid PRIMARY KEY NOT NULL,
	"section_id" uuid NOT NULL,
	"organization_id" uuid NOT NULL,
	"title" text NOT NULL,
	"description" text,
	"sort_order" integer DEFAULT 0 NOT NULL,
	"created_at" timestamp DEFAULT now() NOT NULL,
	"updated_at" timestamp DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE TABLE "training_question_options" (
	"id" uuid PRIMARY KEY NOT NULL,
	"question_id" uuid NOT NULL,
	"text" text NOT NULL,
	"is_correct" boolean DEFAULT false NOT NULL,
	"sort_order" integer DEFAULT 0 NOT NULL,
	"created_at" timestamp DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE TABLE "training_questions" (
	"id" uuid PRIMARY KEY NOT NULL,
	"quiz_id" uuid NOT NULL,
	"text" text NOT NULL,
	"sort_order" integer DEFAULT 0 NOT NULL,
	"created_at" timestamp DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE TABLE "training_quiz_attempts" (
	"id" uuid PRIMARY KEY NOT NULL,
	"quiz_id" uuid NOT NULL,
	"user_id" uuid NOT NULL,
	"organization_id" uuid NOT NULL,
	"score" integer NOT NULL,
	"passed" boolean DEFAULT false NOT NULL,
	"answers" jsonb NOT NULL,
	"created_at" timestamp DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE TABLE "training_quizzes" (
	"id" uuid PRIMARY KEY NOT NULL,
	"lecture_id" uuid NOT NULL,
	"organization_id" uuid NOT NULL,
	"title" text NOT NULL,
	"description" text,
	"passing_score" integer NOT NULL,
	"created_at" timestamp DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE TABLE "training_sections" (
	"id" uuid PRIMARY KEY NOT NULL,
	"course_id" uuid NOT NULL,
	"organization_id" uuid NOT NULL,
	"title" text NOT NULL,
	"description" text,
	"sort_order" integer DEFAULT 0 NOT NULL,
	"created_at" timestamp DEFAULT now() NOT NULL,
	"updated_at" timestamp DEFAULT now() NOT NULL
);
--> statement-breakpoint
ALTER TABLE "audit_events" ADD CONSTRAINT "audit_events_organization_id_organizations_id_fk" FOREIGN KEY ("organization_id") REFERENCES "public"."organizations"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "audit_events" ADD CONSTRAINT "audit_events_actor_user_id_users_id_fk" FOREIGN KEY ("actor_user_id") REFERENCES "public"."users"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "accounts" ADD CONSTRAINT "accounts_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "invitations" ADD CONSTRAINT "invitations_organization_id_organizations_id_fk" FOREIGN KEY ("organization_id") REFERENCES "public"."organizations"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "invitations" ADD CONSTRAINT "invitations_inviter_id_users_id_fk" FOREIGN KEY ("inviter_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "members" ADD CONSTRAINT "members_organization_id_organizations_id_fk" FOREIGN KEY ("organization_id") REFERENCES "public"."organizations"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "members" ADD CONSTRAINT "members_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "sessions" ADD CONSTRAINT "sessions_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "two_factor" ADD CONSTRAINT "two_factor_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "inventory" ADD CONSTRAINT "inventory_variant_id_product_variants_id_fk" FOREIGN KEY ("variant_id") REFERENCES "public"."product_variants"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "inventory_movements" ADD CONSTRAINT "inventory_movements_variant_id_product_variants_id_fk" FOREIGN KEY ("variant_id") REFERENCES "public"."product_variants"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "inventory_movements" ADD CONSTRAINT "inventory_movements_created_by_users_id_fk" FOREIGN KEY ("created_by") REFERENCES "public"."users"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "order_items" ADD CONSTRAINT "order_items_order_id_orders_id_fk" FOREIGN KEY ("order_id") REFERENCES "public"."orders"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "order_items" ADD CONSTRAINT "order_items_variant_id_product_variants_id_fk" FOREIGN KEY ("variant_id") REFERENCES "public"."product_variants"("id") ON DELETE restrict ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "orders" ADD CONSTRAINT "orders_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "product_images" ADD CONSTRAINT "product_images_product_id_products_id_fk" FOREIGN KEY ("product_id") REFERENCES "public"."products"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "product_variants" ADD CONSTRAINT "product_variants_product_id_products_id_fk" FOREIGN KEY ("product_id") REFERENCES "public"."products"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "products" ADD CONSTRAINT "products_collection_id_collections_id_fk" FOREIGN KEY ("collection_id") REFERENCES "public"."collections"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "powersync_upload_operations" ADD CONSTRAINT "powersync_upload_operations_organization_id_organizations_id_fk" FOREIGN KEY ("organization_id") REFERENCES "public"."organizations"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "powersync_upload_operations" ADD CONSTRAINT "powersync_upload_operations_created_by_user_id_users_id_fk" FOREIGN KEY ("created_by_user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "concessions" ADD CONSTRAINT "concessions_organization_id_organizations_id_fk" FOREIGN KEY ("organization_id") REFERENCES "public"."organizations"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "concessions" ADD CONSTRAINT "concessions_created_by_user_id_users_id_fk" FOREIGN KEY ("created_by_user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "installations" ADD CONSTRAINT "installations_concession_id_concessions_id_fk" FOREIGN KEY ("concession_id") REFERENCES "public"."concessions"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "installations" ADD CONSTRAINT "installations_organization_id_organizations_id_fk" FOREIGN KEY ("organization_id") REFERENCES "public"."organizations"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "installations" ADD CONSTRAINT "installations_created_by_user_id_users_id_fk" FOREIGN KEY ("created_by_user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "lab_oil_analyses" ADD CONSTRAINT "lab_oil_analyses_tank_id_tanks_id_fk" FOREIGN KEY ("tank_id") REFERENCES "public"."tanks"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "lab_oil_analyses" ADD CONSTRAINT "lab_oil_analyses_organization_id_organizations_id_fk" FOREIGN KEY ("organization_id") REFERENCES "public"."organizations"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "lab_oil_analyses" ADD CONSTRAINT "lab_oil_analyses_created_by_user_id_users_id_fk" FOREIGN KEY ("created_by_user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "measurement_equipments" ADD CONSTRAINT "measurement_equipments_organization_id_organizations_id_fk" FOREIGN KEY ("organization_id") REFERENCES "public"."organizations"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "measurement_equipments" ADD CONSTRAINT "measurement_equipments_created_by_user_id_users_id_fk" FOREIGN KEY ("created_by_user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "tank_calibration_points" ADD CONSTRAINT "tank_calibration_points_calibration_id_tank_calibrations_id_fk" FOREIGN KEY ("calibration_id") REFERENCES "public"."tank_calibrations"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "tank_calibrations" ADD CONSTRAINT "tank_calibrations_tank_id_tanks_id_fk" FOREIGN KEY ("tank_id") REFERENCES "public"."tanks"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "tank_calibrations" ADD CONSTRAINT "tank_calibrations_organization_id_organizations_id_fk" FOREIGN KEY ("organization_id") REFERENCES "public"."organizations"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "tank_calibrations" ADD CONSTRAINT "tank_calibrations_created_by_user_id_users_id_fk" FOREIGN KEY ("created_by_user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "tank_day_bulletins" ADD CONSTRAINT "tank_day_bulletins_tank_id_tanks_id_fk" FOREIGN KEY ("tank_id") REFERENCES "public"."tanks"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "tank_day_bulletins" ADD CONSTRAINT "tank_day_bulletins_approved_by_user_id_users_id_fk" FOREIGN KEY ("approved_by_user_id") REFERENCES "public"."users"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "tank_day_bulletins" ADD CONSTRAINT "tank_day_bulletins_reopened_by_user_id_users_id_fk" FOREIGN KEY ("reopened_by_user_id") REFERENCES "public"."users"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "tank_day_bulletins" ADD CONSTRAINT "tank_day_bulletins_organization_id_organizations_id_fk" FOREIGN KEY ("organization_id") REFERENCES "public"."organizations"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "tank_transfers" ADD CONSTRAINT "tank_transfers_tank_id_tanks_id_fk" FOREIGN KEY ("tank_id") REFERENCES "public"."tanks"("id") ON DELETE restrict ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "tank_transfers" ADD CONSTRAINT "tank_transfers_organization_id_organizations_id_fk" FOREIGN KEY ("organization_id") REFERENCES "public"."organizations"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "tank_transfers" ADD CONSTRAINT "tank_transfers_tank_calibration_id_tank_calibrations_id_fk" FOREIGN KEY ("tank_calibration_id") REFERENCES "public"."tank_calibrations"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "tank_transfers" ADD CONSTRAINT "tank_transfers_lab_oil_analysis_id_lab_oil_analyses_id_fk" FOREIGN KEY ("lab_oil_analysis_id") REFERENCES "public"."lab_oil_analyses"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "tank_transfers" ADD CONSTRAINT "tank_transfers_tankage_id_tankages_id_fk" FOREIGN KEY ("tankage_id") REFERENCES "public"."tankages"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "tank_transfers" ADD CONSTRAINT "tank_transfers_created_by_user_id_users_id_fk" FOREIGN KEY ("created_by_user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "tankages" ADD CONSTRAINT "tankages_tank_id_tanks_id_fk" FOREIGN KEY ("tank_id") REFERENCES "public"."tanks"("id") ON DELETE restrict ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "tankages" ADD CONSTRAINT "tankages_concession_id_concessions_id_fk" FOREIGN KEY ("concession_id") REFERENCES "public"."concessions"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "tankages" ADD CONSTRAINT "tankages_installation_id_installations_id_fk" FOREIGN KEY ("installation_id") REFERENCES "public"."installations"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "tankages" ADD CONSTRAINT "tankages_measurement_equipment_id_measurement_equipments_id_fk" FOREIGN KEY ("measurement_equipment_id") REFERENCES "public"."measurement_equipments"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "tankages" ADD CONSTRAINT "tankages_operator_user_id_users_id_fk" FOREIGN KEY ("operator_user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "tankages" ADD CONSTRAINT "tankages_tank_calibration_id_tank_calibrations_id_fk" FOREIGN KEY ("tank_calibration_id") REFERENCES "public"."tank_calibrations"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "tankages" ADD CONSTRAINT "tankages_lab_oil_analysis_id_lab_oil_analyses_id_fk" FOREIGN KEY ("lab_oil_analysis_id") REFERENCES "public"."lab_oil_analyses"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "tankages" ADD CONSTRAINT "tankages_organization_id_organizations_id_fk" FOREIGN KEY ("organization_id") REFERENCES "public"."organizations"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "tankages" ADD CONSTRAINT "tankages_created_by_user_id_users_id_fk" FOREIGN KEY ("created_by_user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "tanks" ADD CONSTRAINT "tanks_concession_id_concessions_id_fk" FOREIGN KEY ("concession_id") REFERENCES "public"."concessions"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "tanks" ADD CONSTRAINT "tanks_installation_id_installations_id_fk" FOREIGN KEY ("installation_id") REFERENCES "public"."installations"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "tanks" ADD CONSTRAINT "tanks_measurement_equipment_id_measurement_equipments_id_fk" FOREIGN KEY ("measurement_equipment_id") REFERENCES "public"."measurement_equipments"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "tanks" ADD CONSTRAINT "tanks_organization_id_organizations_id_fk" FOREIGN KEY ("organization_id") REFERENCES "public"."organizations"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "tanks" ADD CONSTRAINT "tanks_created_by_user_id_users_id_fk" FOREIGN KEY ("created_by_user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "todo" ADD CONSTRAINT "todo_organization_id_organizations_id_fk" FOREIGN KEY ("organization_id") REFERENCES "public"."organizations"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "training_courses" ADD CONSTRAINT "training_courses_organization_id_organizations_id_fk" FOREIGN KEY ("organization_id") REFERENCES "public"."organizations"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "training_courses" ADD CONSTRAINT "training_courses_created_by_user_id_users_id_fk" FOREIGN KEY ("created_by_user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "training_enrollments" ADD CONSTRAINT "training_enrollments_course_id_training_courses_id_fk" FOREIGN KEY ("course_id") REFERENCES "public"."training_courses"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "training_enrollments" ADD CONSTRAINT "training_enrollments_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "training_enrollments" ADD CONSTRAINT "training_enrollments_organization_id_organizations_id_fk" FOREIGN KEY ("organization_id") REFERENCES "public"."organizations"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "training_lecture_progress" ADD CONSTRAINT "training_lecture_progress_lecture_id_training_lectures_id_fk" FOREIGN KEY ("lecture_id") REFERENCES "public"."training_lectures"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "training_lecture_progress" ADD CONSTRAINT "training_lecture_progress_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "training_lecture_progress" ADD CONSTRAINT "training_lecture_progress_organization_id_organizations_id_fk" FOREIGN KEY ("organization_id") REFERENCES "public"."organizations"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "training_lectures" ADD CONSTRAINT "training_lectures_module_id_training_modules_id_fk" FOREIGN KEY ("module_id") REFERENCES "public"."training_modules"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "training_lectures" ADD CONSTRAINT "training_lectures_organization_id_organizations_id_fk" FOREIGN KEY ("organization_id") REFERENCES "public"."organizations"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "training_modules" ADD CONSTRAINT "training_modules_section_id_training_sections_id_fk" FOREIGN KEY ("section_id") REFERENCES "public"."training_sections"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "training_modules" ADD CONSTRAINT "training_modules_organization_id_organizations_id_fk" FOREIGN KEY ("organization_id") REFERENCES "public"."organizations"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "training_question_options" ADD CONSTRAINT "training_question_options_question_id_training_questions_id_fk" FOREIGN KEY ("question_id") REFERENCES "public"."training_questions"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "training_questions" ADD CONSTRAINT "training_questions_quiz_id_training_quizzes_id_fk" FOREIGN KEY ("quiz_id") REFERENCES "public"."training_quizzes"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "training_quiz_attempts" ADD CONSTRAINT "training_quiz_attempts_quiz_id_training_quizzes_id_fk" FOREIGN KEY ("quiz_id") REFERENCES "public"."training_quizzes"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "training_quiz_attempts" ADD CONSTRAINT "training_quiz_attempts_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "training_quiz_attempts" ADD CONSTRAINT "training_quiz_attempts_organization_id_organizations_id_fk" FOREIGN KEY ("organization_id") REFERENCES "public"."organizations"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "training_quizzes" ADD CONSTRAINT "training_quizzes_lecture_id_training_lectures_id_fk" FOREIGN KEY ("lecture_id") REFERENCES "public"."training_lectures"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "training_quizzes" ADD CONSTRAINT "training_quizzes_organization_id_organizations_id_fk" FOREIGN KEY ("organization_id") REFERENCES "public"."organizations"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "training_sections" ADD CONSTRAINT "training_sections_course_id_training_courses_id_fk" FOREIGN KEY ("course_id") REFERENCES "public"."training_courses"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "training_sections" ADD CONSTRAINT "training_sections_organization_id_organizations_id_fk" FOREIGN KEY ("organization_id") REFERENCES "public"."organizations"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "audit_events_org_aggregate_occurred_at_idx" ON "audit_events" USING btree ("organization_id","aggregate_type","aggregate_id","occurred_at");--> statement-breakpoint
CREATE INDEX "audit_events_organization_id_idx" ON "audit_events" USING btree ("organization_id");--> statement-breakpoint
CREATE INDEX "audit_events_entity_type_entity_id_idx" ON "audit_events" USING btree ("entity_type","entity_id");--> statement-breakpoint
CREATE INDEX "audit_events_actor_user_id_idx" ON "audit_events" USING btree ("actor_user_id");--> statement-breakpoint
CREATE INDEX "accounts_user_id_idx" ON "accounts" USING btree ("user_id");--> statement-breakpoint
CREATE INDEX "invitations_organization_id_idx" ON "invitations" USING btree ("organization_id");--> statement-breakpoint
CREATE INDEX "invitations_email_idx" ON "invitations" USING btree ("email");--> statement-breakpoint
CREATE INDEX "members_organization_id_idx" ON "members" USING btree ("organization_id");--> statement-breakpoint
CREATE INDEX "members_user_id_idx" ON "members" USING btree ("user_id");--> statement-breakpoint
CREATE UNIQUE INDEX "organizations_slug_uidx" ON "organizations" USING btree ("slug");--> statement-breakpoint
CREATE INDEX "sessions_user_id_idx" ON "sessions" USING btree ("user_id");--> statement-breakpoint
CREATE INDEX "two_factor_secret_idx" ON "two_factor" USING btree ("secret");--> statement-breakpoint
CREATE INDEX "two_factor_user_id_idx" ON "two_factor" USING btree ("user_id");--> statement-breakpoint
CREATE INDEX "verifications_identifier_idx" ON "verifications" USING btree ("identifier");--> statement-breakpoint
CREATE UNIQUE INDEX "collections_slug_uidx" ON "collections" USING btree ("slug");--> statement-breakpoint
CREATE UNIQUE INDEX "inventory_variant_id_uidx" ON "inventory" USING btree ("variant_id");--> statement-breakpoint
CREATE INDEX "inventory_movements_variant_id_idx" ON "inventory_movements" USING btree ("variant_id");--> statement-breakpoint
CREATE INDEX "inventory_movements_created_at_idx" ON "inventory_movements" USING btree ("created_at");--> statement-breakpoint
CREATE INDEX "order_items_order_id_idx" ON "order_items" USING btree ("order_id");--> statement-breakpoint
CREATE INDEX "orders_user_id_idx" ON "orders" USING btree ("user_id");--> statement-breakpoint
CREATE INDEX "orders_status_idx" ON "orders" USING btree ("status");--> statement-breakpoint
CREATE INDEX "orders_created_at_idx" ON "orders" USING btree ("created_at");--> statement-breakpoint
CREATE INDEX "product_images_product_id_idx" ON "product_images" USING btree ("product_id");--> statement-breakpoint
CREATE UNIQUE INDEX "product_variants_sku_uidx" ON "product_variants" USING btree ("sku");--> statement-breakpoint
CREATE INDEX "product_variants_product_id_idx" ON "product_variants" USING btree ("product_id");--> statement-breakpoint
CREATE UNIQUE INDEX "products_slug_uidx" ON "products" USING btree ("slug");--> statement-breakpoint
CREATE INDEX "products_collection_id_idx" ON "products" USING btree ("collection_id");--> statement-breakpoint
CREATE INDEX "products_active_idx" ON "products" USING btree ("active");--> statement-breakpoint
CREATE UNIQUE INDEX "curriculum_file_key_idx" ON "curriculum" USING btree ("file_key");--> statement-breakpoint
CREATE INDEX "powersync_upload_operations_organization_id_idx" ON "powersync_upload_operations" USING btree ("organization_id");--> statement-breakpoint
CREATE INDEX "powersync_upload_operations_updated_at_idx" ON "powersync_upload_operations" USING btree ("updated_at");--> statement-breakpoint
CREATE INDEX "concessions_organization_id_idx" ON "concessions" USING btree ("organization_id");--> statement-breakpoint
CREATE INDEX "concessions_created_by_user_id_idx" ON "concessions" USING btree ("created_by_user_id");--> statement-breakpoint
CREATE INDEX "installations_organization_id_idx" ON "installations" USING btree ("organization_id");--> statement-breakpoint
CREATE INDEX "installations_concession_id_idx" ON "installations" USING btree ("concession_id");--> statement-breakpoint
CREATE INDEX "installations_created_by_user_id_idx" ON "installations" USING btree ("created_by_user_id");--> statement-breakpoint
CREATE INDEX "lab_oil_analyses_organization_id_idx" ON "lab_oil_analyses" USING btree ("organization_id");--> statement-breakpoint
CREATE INDEX "lab_oil_analyses_tank_id_idx" ON "lab_oil_analyses" USING btree ("tank_id");--> statement-breakpoint
CREATE INDEX "lab_oil_analyses_tank_id_collected_at_idx" ON "lab_oil_analyses" USING btree ("tank_id","collected_at");--> statement-breakpoint
CREATE INDEX "lab_oil_analyses_created_by_user_id_idx" ON "lab_oil_analyses" USING btree ("created_by_user_id");--> statement-breakpoint
CREATE UNIQUE INDEX "measurement_equipments_organization_id_code_uidx" ON "measurement_equipments" USING btree ("organization_id","code");--> statement-breakpoint
CREATE INDEX "measurement_equipments_organization_id_idx" ON "measurement_equipments" USING btree ("organization_id");--> statement-breakpoint
CREATE INDEX "measurement_equipments_created_by_user_id_idx" ON "measurement_equipments" USING btree ("created_by_user_id");--> statement-breakpoint
CREATE UNIQUE INDEX "tank_calibration_points_calibration_id_height_cm_uidx" ON "tank_calibration_points" USING btree ("calibration_id","height_cm");--> statement-breakpoint
CREATE INDEX "tank_calibration_points_calibration_id_idx" ON "tank_calibration_points" USING btree ("calibration_id");--> statement-breakpoint
CREATE INDEX "tank_calibrations_organization_id_idx" ON "tank_calibrations" USING btree ("organization_id");--> statement-breakpoint
CREATE INDEX "tank_calibrations_tank_id_idx" ON "tank_calibrations" USING btree ("tank_id");--> statement-breakpoint
CREATE INDEX "tank_calibrations_tank_id_valid_from_idx" ON "tank_calibrations" USING btree ("tank_id","valid_from");--> statement-breakpoint
CREATE INDEX "tank_calibrations_created_by_user_id_idx" ON "tank_calibrations" USING btree ("created_by_user_id");--> statement-breakpoint
CREATE UNIQUE INDEX "tank_day_bulletins_org_tank_day_uidx" ON "tank_day_bulletins" USING btree ("organization_id","tank_id","operational_day");--> statement-breakpoint
CREATE INDEX "tank_day_bulletins_organization_id_idx" ON "tank_day_bulletins" USING btree ("organization_id");--> statement-breakpoint
CREATE INDEX "tank_day_bulletins_tank_id_idx" ON "tank_day_bulletins" USING btree ("tank_id");--> statement-breakpoint
CREATE INDEX "tank_day_bulletins_tank_id_operational_day_idx" ON "tank_day_bulletins" USING btree ("tank_id","operational_day");--> statement-breakpoint
CREATE UNIQUE INDEX "tank_transfers_tankage_id_uidx" ON "tank_transfers" USING btree ("tankage_id");--> statement-breakpoint
CREATE INDEX "tank_transfers_organization_id_idx" ON "tank_transfers" USING btree ("organization_id");--> statement-breakpoint
CREATE INDEX "tank_transfers_tank_id_idx" ON "tank_transfers" USING btree ("tank_id");--> statement-breakpoint
CREATE INDEX "tank_transfers_transferred_at_idx" ON "tank_transfers" USING btree ("transferred_at");--> statement-breakpoint
CREATE INDEX "tank_transfers_org_tank_operational_day_idx" ON "tank_transfers" USING btree ("organization_id","tank_id","operational_day");--> statement-breakpoint
CREATE INDEX "tank_transfers_created_by_user_id_idx" ON "tank_transfers" USING btree ("created_by_user_id");--> statement-breakpoint
CREATE INDEX "tankages_organization_id_idx" ON "tankages" USING btree ("organization_id");--> statement-breakpoint
CREATE INDEX "tankages_tank_id_idx" ON "tankages" USING btree ("tank_id");--> statement-breakpoint
CREATE INDEX "tankages_concession_id_idx" ON "tankages" USING btree ("concession_id");--> statement-breakpoint
CREATE INDEX "tankages_installation_id_idx" ON "tankages" USING btree ("installation_id");--> statement-breakpoint
CREATE INDEX "tankages_measurement_equipment_id_idx" ON "tankages" USING btree ("measurement_equipment_id");--> statement-breakpoint
CREATE INDEX "tankages_operator_user_id_idx" ON "tankages" USING btree ("operator_user_id");--> statement-breakpoint
CREATE INDEX "tankages_measured_at_idx" ON "tankages" USING btree ("measured_at");--> statement-breakpoint
CREATE INDEX "tankages_tank_id_measured_at_idx" ON "tankages" USING btree ("tank_id","measured_at");--> statement-breakpoint
CREATE INDEX "tankages_org_tank_operational_day_idx" ON "tankages" USING btree ("organization_id","tank_id","operational_day");--> statement-breakpoint
CREATE INDEX "tankages_created_by_user_id_idx" ON "tankages" USING btree ("created_by_user_id");--> statement-breakpoint
CREATE UNIQUE INDEX "tanks_organization_id_tag_uidx" ON "tanks" USING btree ("organization_id","tag");--> statement-breakpoint
CREATE INDEX "tanks_organization_id_idx" ON "tanks" USING btree ("organization_id");--> statement-breakpoint
CREATE INDEX "tanks_concession_id_idx" ON "tanks" USING btree ("concession_id");--> statement-breakpoint
CREATE INDEX "tanks_installation_id_idx" ON "tanks" USING btree ("installation_id");--> statement-breakpoint
CREATE INDEX "tanks_measurement_equipment_id_idx" ON "tanks" USING btree ("measurement_equipment_id");--> statement-breakpoint
CREATE INDEX "tanks_created_by_user_id_idx" ON "tanks" USING btree ("created_by_user_id");--> statement-breakpoint
CREATE INDEX "training_courses_organization_id_idx" ON "training_courses" USING btree ("organization_id");--> statement-breakpoint
CREATE INDEX "training_courses_created_by_user_id_idx" ON "training_courses" USING btree ("created_by_user_id");--> statement-breakpoint
CREATE UNIQUE INDEX "training_enrollments_course_user_uidx" ON "training_enrollments" USING btree ("course_id","user_id");--> statement-breakpoint
CREATE INDEX "training_enrollments_course_id_idx" ON "training_enrollments" USING btree ("course_id");--> statement-breakpoint
CREATE INDEX "training_enrollments_user_id_idx" ON "training_enrollments" USING btree ("user_id");--> statement-breakpoint
CREATE INDEX "training_enrollments_organization_id_idx" ON "training_enrollments" USING btree ("organization_id");--> statement-breakpoint
CREATE UNIQUE INDEX "training_lecture_progress_lecture_user_uidx" ON "training_lecture_progress" USING btree ("lecture_id","user_id");--> statement-breakpoint
CREATE INDEX "training_lecture_progress_lecture_id_idx" ON "training_lecture_progress" USING btree ("lecture_id");--> statement-breakpoint
CREATE INDEX "training_lecture_progress_user_id_idx" ON "training_lecture_progress" USING btree ("user_id");--> statement-breakpoint
CREATE INDEX "training_lecture_progress_organization_id_idx" ON "training_lecture_progress" USING btree ("organization_id");--> statement-breakpoint
CREATE INDEX "training_lectures_module_id_idx" ON "training_lectures" USING btree ("module_id");--> statement-breakpoint
CREATE INDEX "training_lectures_organization_id_idx" ON "training_lectures" USING btree ("organization_id");--> statement-breakpoint
CREATE INDEX "training_modules_section_id_idx" ON "training_modules" USING btree ("section_id");--> statement-breakpoint
CREATE INDEX "training_modules_organization_id_idx" ON "training_modules" USING btree ("organization_id");--> statement-breakpoint
CREATE INDEX "training_question_options_question_id_idx" ON "training_question_options" USING btree ("question_id");--> statement-breakpoint
CREATE INDEX "training_questions_quiz_id_idx" ON "training_questions" USING btree ("quiz_id");--> statement-breakpoint
CREATE INDEX "training_quiz_attempts_quiz_id_idx" ON "training_quiz_attempts" USING btree ("quiz_id");--> statement-breakpoint
CREATE INDEX "training_quiz_attempts_user_id_idx" ON "training_quiz_attempts" USING btree ("user_id");--> statement-breakpoint
CREATE INDEX "training_quiz_attempts_organization_id_idx" ON "training_quiz_attempts" USING btree ("organization_id");--> statement-breakpoint
CREATE UNIQUE INDEX "training_quizzes_lecture_id_uidx" ON "training_quizzes" USING btree ("lecture_id");--> statement-breakpoint
CREATE INDEX "training_quizzes_organization_id_idx" ON "training_quizzes" USING btree ("organization_id");--> statement-breakpoint
CREATE INDEX "training_sections_course_id_idx" ON "training_sections" USING btree ("course_id");--> statement-breakpoint
CREATE INDEX "training_sections_organization_id_idx" ON "training_sections" USING btree ("organization_id");