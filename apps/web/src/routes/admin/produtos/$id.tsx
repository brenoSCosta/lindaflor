import { createFileRoute, Link } from "@tanstack/react-router";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Textarea } from "@/components/ui/textarea";
import {
  productCategories,
  productCategoryLabels,
} from "@lindaflor/shared/enums/commerce";
import { formatCurrency } from "@/lib/format";
import { orpc } from "@/lib/orpc";

export const Route = createFileRoute("/admin/produtos/$id")({
  component: AdminEditProductPage,
});

function AdminEditProductPage() {
  const { id } = Route.useParams();
  const queryClient = useQueryClient();
  const [imageFile, setImageFile] = useState<File | null>(null);

  const productsQuery = useQuery(
    orpc.commerce.admin.listProducts.queryOptions({ input: undefined }),
  );

  const product = productsQuery.data?.data.find((item) => item.id === id);

  const detailQuery = useQuery({
    ...orpc.commerce.store.getProduct.queryOptions({
      input: { slug: product?.slug ?? "" },
    }),
    enabled: Boolean(product?.slug),
  });

  const [form, setForm] = useState({
    name: "",
    slug: "",
    description: "",
    price_in_cents: "",
    category: "biquini" as (typeof productCategories)[number],
    featured: false,
    active: true,
  });

  useEffect(() => {
    if (!product) {
      return;
    }
    setForm({
      name: product.name,
      slug: product.slug,
      description: product.description ?? "",
      price_in_cents: String(product.price_in_cents / 100),
      category: product.category,
      featured: product.featured,
      active: product.active,
    });
  }, [product]);

  const updateMutation = useMutation(
    orpc.commerce.admin.updateProduct.mutationOptions({
      onSuccess: async () => {
        if (imageFile) {
          await uploadMutation.mutateAsync({ product_id: id, file: imageFile });
        }
        toast.success("Produto atualizado");
        await queryClient.invalidateQueries();
      },
      onError: (error) => toast.error(error.message),
    }),
  );

  const uploadMutation = useMutation(
    orpc.commerce.admin.uploadProductImage.mutationOptions(),
  );

  const deleteImageMutation = useMutation(
    orpc.commerce.admin.deleteProductImage.mutationOptions({
      onSuccess: async () => {
        await queryClient.invalidateQueries();
        toast.success("Imagem removida");
      },
    }),
  );

  if (productsQuery.isLoading) {
    return <p>Carregando...</p>;
  }

  if (!product) {
    return <p className="text-red-600">Produto não encontrado.</p>;
  }

  function handleSubmit(event: React.FormEvent) {
    event.preventDefault();
    const price = Math.round(Number(form.price_in_cents.replace(",", ".")) * 100);
    updateMutation.mutate({
      id,
      name: form.name,
      slug: form.slug,
      description: form.description || undefined,
      price_in_cents: price,
      category: form.category,
      featured: form.featured,
      active: form.active,
    });
  }

  return (
    <div className="space-y-6">
      <div>
        <p className="text-sm text-stone-500">
          <Link to="/admin/produtos">Produtos</Link> / {product.name}
        </p>
        <h2 className="text-2xl font-semibold">Editar produto</h2>
      </div>

      <form
        onSubmit={handleSubmit}
        className="space-y-6 rounded-xl border bg-white p-6"
      >
        <div className="grid gap-4 md:grid-cols-2">
          <div className="space-y-2">
            <Label htmlFor="name">Nome</Label>
            <Input
              id="name"
              value={form.name}
              onChange={(e) => setForm((c) => ({ ...c, name: e.target.value }))}
            />
          </div>
          <div className="space-y-2">
            <Label htmlFor="slug">Slug</Label>
            <Input
              id="slug"
              value={form.slug}
              onChange={(e) => setForm((c) => ({ ...c, slug: e.target.value }))}
            />
          </div>
          <div className="space-y-2 md:col-span-2">
            <Label htmlFor="description">Descrição</Label>
            <Textarea
              id="description"
              value={form.description}
              onChange={(e) =>
                setForm((c) => ({ ...c, description: e.target.value }))
              }
            />
          </div>
          <div className="space-y-2">
            <Label htmlFor="price">Preço (R$)</Label>
            <Input
              id="price"
              value={form.price_in_cents}
              onChange={(e) =>
                setForm((c) => ({ ...c, price_in_cents: e.target.value }))
              }
            />
          </div>
          <div className="space-y-2">
            <Label htmlFor="category">Categoria</Label>
            <select
              id="category"
              value={form.category}
              onChange={(e) =>
                setForm((c) => ({
                  ...c,
                  category: e.target.value as (typeof productCategories)[number],
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
          <label className="flex items-center gap-2 text-sm">
            <input
              type="checkbox"
              checked={form.featured}
              onChange={(e) =>
                setForm((c) => ({ ...c, featured: e.target.checked }))
              }
            />
            Destaque
          </label>
          <label className="flex items-center gap-2 text-sm">
            <input
              type="checkbox"
              checked={form.active}
              onChange={(e) =>
                setForm((c) => ({ ...c, active: e.target.checked }))
              }
            />
            Ativo na loja
          </label>
        </div>

        <div className="space-y-3">
          <Label>Imagens</Label>
          <div className="grid grid-cols-2 gap-3 md:grid-cols-4">
            {detailQuery.data?.images.map((image) => (
              <div key={image.id} className="space-y-2">
                <img
                  src={image.url}
                  alt={image.alt ?? product.name}
                  className="aspect-square w-full object-cover"
                />
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  className="w-full"
                  onClick={() => deleteImageMutation.mutate({ id: image.id })}
                >
                  Remover
                </Button>
              </div>
            ))}
          </div>
          <Input
            type="file"
            accept="image/jpeg,image/png,image/webp"
            onChange={(e) => setImageFile(e.target.files?.[0] ?? null)}
          />
        </div>

        <p className="text-sm text-stone-500">
          Preço base: {formatCurrency(product.price_in_cents)} ·{" "}
          {product.variant_count} variantes · {product.available_total} disponíveis
        </p>

        <Button type="submit" disabled={updateMutation.isPending}>
          {updateMutation.isPending ? "Salvando..." : "Salvar alterações"}
        </Button>
      </form>
    </div>
  );
}
