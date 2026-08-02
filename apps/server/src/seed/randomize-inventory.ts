import { randomizeInventory } from "@/seed/seeders/randomize-inventory";

const count = await randomizeInventory();
console.log(`Estoque aleatório aplicado em ${count} variantes.`);
