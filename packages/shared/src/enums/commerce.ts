export const productCategories = [
  "biquini",
  "maio",
  "saida_praia",
  "acessorio",
] as const;

export const productCategoryLabels: Record<
  (typeof productCategories)[number],
  string
> = {
  biquini: "Biquíni",
  maio: "Maiô",
  saida_praia: "Saída de praia",
  acessorio: "Acessório",
};

export const productSizes = ["pp", "p", "m", "g", "gg"] as const;

export const productSizeLabels: Record<(typeof productSizes)[number], string> =
  {
    pp: "PP",
    p: "P",
    m: "M",
    g: "G",
    gg: "GG",
  };

export const orderStatuses = [
  "pending_payment",
  "paid",
  "processing",
  "shipped",
  "delivered",
  "cancelled",
] as const;

export const orderStatusLabels: Record<(typeof orderStatuses)[number], string> =
  {
    pending_payment: "Aguardando pagamento",
    paid: "Pago",
    processing: "Em separação",
    shipped: "Enviado",
    delivered: "Entregue",
    cancelled: "Cancelado",
  };
