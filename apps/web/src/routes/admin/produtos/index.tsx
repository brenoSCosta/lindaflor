import { productCategoryLabels } from "@lindaflor/shared/enums/commerce";
import { useQuery } from "@tanstack/react-query";
import { createFileRoute, Link } from "@tanstack/react-router";

import { Button } from "@/components/ui/button";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { formatCurrency } from "@/lib/format";
import { orpc } from "@/lib/orpc";

export const Route = createFileRoute("/admin/produtos/")({
  component: AdminProductsPage,
});

function AdminProductsPage() {
  const productsQuery = useQuery(
    orpc.commerce.admin.listProducts.queryOptions({ input: undefined }),
  );

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between gap-4">
        <div>
          <h2 className="text-2xl font-semibold">Produtos</h2>
          <p className="text-stone-600">
            Lista de produtos cadastrados no catálogo.
          </p>
        </div>
        <Link to="/admin/produtos/novo">
          <Button>Novo produto</Button>
        </Link>
      </div>

      {productsQuery.isLoading ? (
        <p>Carregando…</p>
      ) : productsQuery.isError ? (
        <p className="text-red-600">
          Faça login como admin para ver os produtos.
        </p>
      ) : (
        <div className="overflow-hidden rounded-xl border bg-white">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Nome</TableHead>
                <TableHead>Categoria</TableHead>
                <TableHead>Preço</TableHead>
                <TableHead>Variantes</TableHead>
                <TableHead>Disponível</TableHead>
                <TableHead>Status</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {productsQuery.data?.data.map((product) => (
                <TableRow key={product.id}>
                  <TableCell className="font-medium">
                    <Link
                      to="/admin/produtos/$id"
                      params={{ id: product.id }}
                      className="text-pink-700 underline"
                    >
                      {product.name}
                    </Link>
                  </TableCell>
                  <TableCell>
                    {productCategoryLabels[product.category]}
                  </TableCell>
                  <TableCell>
                    {formatCurrency(product.price_in_cents)}
                  </TableCell>
                  <TableCell>{product.variant_count}</TableCell>
                  <TableCell>{product.available_total}</TableCell>
                  <TableCell>{product.active ? "Ativo" : "Inativo"}</TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </div>
      )}
    </div>
  );
}
