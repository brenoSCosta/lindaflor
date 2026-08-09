import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { orpc } from "@/lib/orpc";

export function VariantThresholdRow({
  variantId,
  label,
  threshold,
  available,
}: {
  variantId: string;
  label: string;
  threshold: number;
  available: number;
}) {
  const queryClient = useQueryClient();
  const [value, setValue] = useState(String(threshold));

  const mutation = useMutation(
    orpc.commerce.admin.updateVariantLowStockThreshold.mutationOptions({
      onSuccess: async () => {
        await queryClient.invalidateQueries();
        toast.success("Limiar atualizado");
      },
      onError: (error) => toast.error(error.message),
    }),
  );

  return (
    <div className="flex flex-wrap items-center gap-3 text-sm">
      <div className="min-w-0 flex-1">
        <p className="font-medium">{label}</p>
        <p className="text-stone-500">{available} disponível</p>
      </div>
      <Input
        className="w-24"
        type="number"
        min={0}
        value={value}
        onChange={(e) => setValue(e.target.value)}
        aria-label={`Limiar de estoque para ${label}`}
      />
      <Button
        type="button"
        size="sm"
        variant="outline"
        disabled={mutation.isPending}
        onClick={() =>
          mutation.mutate({
            variant_id: variantId,
            low_stock_threshold: Number(value),
          })
        }
      >
        Salvar
      </Button>
    </div>
  );
}
