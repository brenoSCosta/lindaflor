import { createFileRoute } from "@tanstack/react-router";
import { useMutation } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { useState } from "react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Textarea } from "@/components/ui/textarea";
import {
  productCategories,
  productCategoryLabels,
  productSizeLabels,
  productSizes,
} from "@lindaflor/shared/enums/commerce";
import { orpc } from "@/lib/orpc";

export const Route = createFileRoute("/admin/produtos/novo")({
  component: AdminNewProductPage,
});

type VariantDraft = {
  sku: string;
  size: (typeof productSizes)[number];
  color: string;
  quantity: number;
};

const defaultVariant = (): VariantDraft => ({
  sku: "",
  size: "m",
  color: "",
  quantity: 0,
});

function AdminNewProductPage() {
  const navigate = useNavigate();
  const [imageFile, setImageFile] = useState<File | null>(null);
  const [form, setForm] = useState({
    name: "",
    slug: "",
    description: "",
    price_in_cents: "",
    category: "biquini" as (typeof productCategories)[number],
    featured: false,
  });
  const [variants, setVariants] = useState<VariantDraft[]>([defaultVariant()]);

  const createMutation = useMutation(
    orpc.commerce.admin.createProduct.mutationOptions({
      onSuccess: async (product) => {
        if (imageFile) {
          try {
            await uploadImageMutation.mutateAsync({
              product_id: product.id,
              file: imageFile,
            });
          } catch {
            toast.error("Produto criado, mas falhou ao enviar imagem");
          }
        }
        toast.success("Produto criado");
        void navigate({ to: "/admin/produtos" });
      },
      onError: (error) => toast.error(error.message),
    }),
  );

  const uploadImageMutation = useMutation(
    orpc.commerce.admin.uploadProductImage.mutationOptions(),
  );

  function updateVariant(index: number, patch: Partial<VariantDraft>) {
    setVariants((current) =>
      current.map((variant, i) =>
        i === index ? { ...variant, ...patch } : variant,
      ),
    );
  }

  function handleSubmit(event: React.FormEvent) {
    event.preventDefault();

    const price = Math.round(Number(form.price_in_cents.replace(",", ".")) * 100);
    if (!price || price <= 0) {
      toast.error("Informe um preço válido");
      return;
    }

    createMutation.mutate({
      name: form.name,
      slug: form.slug,
      description: form.description || undefined,
      price_in_cents: price,
      category: form.category,
      featured: form.featured,
      variants: variants.map((variant) => ({
        sku: variant.sku,
        size: variant.size,
        color: variant.color,
        quantity: variant.quantity,
      })),
    });
  }

  return (
    <div className="space-y-6">
      <div>
        <h2 className="text-2xl font-semibold">Novo produto</h2>
        <p className="text-stone-600">
          Cadastre um produto com variantes e estoque inicial.
        </p>
      </div>

      <form
        onSubmit={handleSubmit}
        className="space-y-8 rounded-xl border bg-white p-6"
      >
        <div className="grid gap-4 md:grid-cols-2">
          <div className="space-y-2">
            <Label htmlFor="name">Nome</Label>
            <Input
              id="name"
              required
              value={form.name}
              onChange={(event) => {
                const name = event.target.value;
                setForm((current) => ({
                  ...current,
                  name,
                  slug:
                    current.slug ||
                    name
                      .toLowerCase()
                      .normalize("NFD")
                      .replace(/[\u0300-\u036f]/g, "")
                      .replace(/[^a-z0-9]+/g, "-")
                      .replace(/(^-|-$)/g, ""),
                }));
              }}
            />
          </div>
          <div className="space-y-2">
            <Label htmlFor="slug">Slug</Label>
            <Input
              id="slug"
              required
              value={form.slug}
              onChange={(event) =>
                setForm((current) => ({ ...current, slug: event.target.value }))
              }
            />
          </div>
          <div className="space-y-2 md:col-span-2">
            <Label htmlFor="description">Descrição</Label>
            <Textarea
              id="description"
              value={form.description}
              onChange={(event) =>
                setForm((current) => ({
                  ...current,
                  description: event.target.value,
                }))
              }
            />
          </div>
          <div className="space-y-2">
            <Label htmlFor="price">Preço (R$)</Label>
            <Input
              id="price"
              required
              placeholder="199.90"
              value={form.price_in_cents}
              onChange={(event) =>
                setForm((current) => ({
                  ...current,
                  price_in_cents: event.target.value,
                }))
              }
            />
          </div>
          <div className="space-y-2">
            <Label htmlFor="category">Categoria</Label>
            <select
              id="category"
              value={form.category}
              onChange={(event) =>
                setForm((current) => ({
                  ...current,
                  category: event.target.value as (typeof productCategories)[number],
                }))
              }
              className="h-10 w-full rounded-md border px-3 text-sm"
            >
              {productCategories.map((category) => (
                <option key={category} value={category}>
                  {productCategoryLabels[category]}
                </option>
              ))}
            </select>
          </div>
          <div className="space-y-2 md:col-span-2">
            <Label htmlFor="image">Imagem principal</Label>
            <Input
              id="image"
              type="file"
              accept="image/jpeg,image/png,image/webp"
              onChange={(event) =>
                setImageFile(event.target.files?.[0] ?? null)
              }
            />
          </div>
          <label className="flex items-center gap-2 text-sm md:col-span-2">
            <input
              type="checkbox"
              checked={form.featured}
              onChange={(event) =>
                setForm((current) => ({
                  ...current,
                  featured: event.target.checked,
                }))
              }
            />
            Destaque na home
          </label>
        </div>

        <div className="space-y-4">
          <div className="flex items-center justify-between">
            <h3 className="font-medium">Variantes</h3>
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={() => setVariants((current) => [...current, defaultVariant()])}
            >
              Adicionar variante
            </Button>
          </div>
          {variants.map((variant, index) => (
            <div
              key={index}
              className="grid gap-3 rounded-lg border p-4 md:grid-cols-4"
            >
              <Input
                placeholder="SKU"
                required
                value={variant.sku}
                onChange={(event) =>
                  updateVariant(index, { sku: event.target.value })
                }
              />
              <select
                value={variant.size}
                onChange={(event) =>
                  updateVariant(index, {
                    size: event.target.value as (typeof productSizes)[number],
                  })
                }
                className="h-10 rounded-md border px-3 text-sm"
              >
                {productSizes.map((size) => (
                  <option key={size} value={size}>
                    {productSizeLabels[size]}
                  </option>
                ))}
              </select>
              <Input
                placeholder="Cor"
                required
                value={variant.color}
                onChange={(event) =>
                  updateVariant(index, { color: event.target.value })
                }
              />
              <Input
                type="number"
                min={0}
                placeholder="Estoque"
                required
                value={variant.quantity}
                onChange={(event) =>
                  updateVariant(index, {
                    quantity: Number(event.target.value),
                  })
                }
              />
            </div>
          ))}
        </div>

        <Button type="submit" disabled={createMutation.isPending}>
          {createMutation.isPending ? "Salvando..." : "Criar produto"}
        </Button>
      </form>
    </div>
  );
}
