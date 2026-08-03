import {
  createContext,
  use,
  useCallback,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import { z } from "zod";

const STORAGE_KEY = "lindaflor-cart-v1";

const cartItemSchema = z.object({
  variantId: z.string(),
  productId: z.string(),
  productSlug: z.string(),
  productName: z.string(),
  variantLabel: z.string(),
  imageUrl: z.string().nullable(),
  unitPriceCents: z.number(),
  quantity: z.number(),
  maxQuantity: z.number(),
});

const cartItemsSchema = z.array(cartItemSchema);

export type CartItem = z.infer<typeof cartItemSchema>;

type CartContextValue = {
  items: CartItem[];
  itemCount: number;
  subtotalCents: number;
  addItem: (item: Omit<CartItem, "quantity">, quantity?: number) => void;
  updateQuantity: (variantId: string, quantity: number) => void;
  removeItem: (variantId: string) => void;
  clearCart: () => void;
};

const CartContext = createContext<CartContextValue | null>(null);

function readStoredCart(): CartItem[] {
  if (typeof window === "undefined") {
    return [];
  }

  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) {
      return [];
    }
    const parsed = cartItemsSchema.safeParse(JSON.parse(raw));
    return parsed.success ? parsed.data : [];
  } catch {
    return [];
  }
}

export function CartProvider({ children }: { children: ReactNode }) {
  const [items, setItems] = useState<CartItem[]>(() => readStoredCart());

  useEffect(() => {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(items));
  }, [items]);

  const addItem = useCallback(
    (item: Omit<CartItem, "quantity">, quantity = 1) => {
      setItems((current) => {
        const existing = current.find(
          (entry) => entry.variantId === item.variantId,
        );

        if (existing) {
          const nextQuantity = Math.min(
            existing.quantity + quantity,
            item.maxQuantity,
          );
          return current.map((entry) =>
            entry.variantId === item.variantId
              ? { ...entry, quantity: nextQuantity }
              : entry,
          );
        }

        return [
          ...current,
          {
            ...item,
            quantity: Math.min(quantity, item.maxQuantity),
          },
        ];
      });
    },
    [],
  );

  const updateQuantity = useCallback((variantId: string, quantity: number) => {
    setItems((current) => {
      const next: CartItem[] = [];

      for (const entry of current) {
        if (entry.variantId !== variantId) {
          next.push(entry);
          continue;
        }

        const nextQuantity = Math.min(Math.max(quantity, 1), entry.maxQuantity);
        if (nextQuantity > 0) {
          next.push({ ...entry, quantity: nextQuantity });
        }
      }

      return next;
    });
  }, []);

  const removeItem = useCallback((variantId: string) => {
    setItems((current) =>
      current.filter((entry) => entry.variantId !== variantId),
    );
  }, []);

  const clearCart = useCallback(() => {
    setItems([]);
  }, []);

  const value = useMemo(() => {
    const itemCount = items.reduce((sum, item) => sum + item.quantity, 0);
    const subtotalCents = items.reduce(
      (sum, item) => sum + item.unitPriceCents * item.quantity,
      0,
    );

    return {
      items,
      itemCount,
      subtotalCents,
      addItem,
      updateQuantity,
      removeItem,
      clearCart,
    };
  }, [addItem, clearCart, items, removeItem, updateQuantity]);

  return <CartContext.Provider value={value}>{children}</CartContext.Provider>;
}

export function useCart() {
  const context = use(CartContext);
  if (!context) {
    throw new Error("useCart must be used within CartProvider");
  }
  return context;
}
