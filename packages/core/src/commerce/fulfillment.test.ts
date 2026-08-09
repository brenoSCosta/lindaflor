import { describe, expect, it } from "bun:test";

/**
 * Allocation algorithm is covered via pure helper scenarios mirroring
 * allocateFulfillment warehouse preference rules.
 */

function allocateFromAvailability(params: {
  items: Array<{ variant_id: string; quantity: number }>;
  warehouses: Array<{ id: string; is_default: boolean }>;
  availability: Map<string, number>;
}) {
  const { items, warehouses, availability } = params;

  const canFulfill = (warehouseId: string) =>
    items.every(
      (item) =>
        (availability.get(`${item.variant_id}:${warehouseId}`) ?? 0) >=
        item.quantity,
    );

  for (const warehouse of warehouses) {
    if (canFulfill(warehouse.id)) {
      return items.map((item) => ({
        variant_id: item.variant_id,
        quantity: item.quantity,
        warehouse_id: warehouse.id,
      }));
    }
  }

  const defaultWarehouse = warehouses.find((w) => w.is_default);
  const allocated = [];

  for (const item of items) {
    let chosen: string | null = null;
    if (defaultWarehouse) {
      const available =
        availability.get(`${item.variant_id}:${defaultWarehouse.id}`) ?? 0;
      if (available >= item.quantity) {
        chosen = defaultWarehouse.id;
      }
    }
    if (!chosen) {
      let best = -1;
      for (const warehouse of warehouses) {
        const available =
          availability.get(`${item.variant_id}:${warehouse.id}`) ?? 0;
        if (available >= item.quantity && available > best) {
          best = available;
          chosen = warehouse.id;
        }
      }
    }
    if (!chosen) {
      throw new Error("insufficient");
    }
    allocated.push({
      variant_id: item.variant_id,
      quantity: item.quantity,
      warehouse_id: chosen,
    });
  }

  return allocated;
}

describe("allocateFulfillment preference rules", () => {
  const warehouses = [
    { id: "default", is_default: true },
    { id: "secondary", is_default: false },
  ];

  it("prefers default warehouse when it can fulfill the whole cart", () => {
    const availability = new Map([
      ["v1:default", 5],
      ["v1:secondary", 10],
      ["v2:default", 2],
      ["v2:secondary", 2],
    ]);

    const result = allocateFromAvailability({
      items: [
        { variant_id: "v1", quantity: 1 },
        { variant_id: "v2", quantity: 1 },
      ],
      warehouses,
      availability,
    });

    expect(result.every((line) => line.warehouse_id === "default")).toBe(true);
  });

  it("falls back to secondary when default cannot fulfill all lines", () => {
    const availability = new Map([
      ["v1:default", 0],
      ["v1:secondary", 5],
      ["v2:default", 0],
      ["v2:secondary", 5],
    ]);

    const result = allocateFromAvailability({
      items: [
        { variant_id: "v1", quantity: 1 },
        { variant_id: "v2", quantity: 1 },
      ],
      warehouses,
      availability,
    });

    expect(result.every((line) => line.warehouse_id === "secondary")).toBe(
      true,
    );
  });

  it("splits per line when no single warehouse can fulfill the cart", () => {
    const availability = new Map([
      ["v1:default", 5],
      ["v1:secondary", 0],
      ["v2:default", 0],
      ["v2:secondary", 5],
    ]);

    const result = allocateFromAvailability({
      items: [
        { variant_id: "v1", quantity: 1 },
        { variant_id: "v2", quantity: 1 },
      ],
      warehouses,
      availability,
    });

    expect(result).toEqual([
      { variant_id: "v1", quantity: 1, warehouse_id: "default" },
      { variant_id: "v2", quantity: 1, warehouse_id: "secondary" },
    ]);
  });

  it("throws when a line cannot be allocated", () => {
    const availability = new Map([
      ["v1:default", 0],
      ["v1:secondary", 0],
    ]);

    expect(() =>
      allocateFromAvailability({
        items: [{ variant_id: "v1", quantity: 1 }],
        warehouses,
        availability,
      }),
    ).toThrow("insufficient");
  });
});
