import { defineConfig } from "tsdown";

export default defineConfig({
  entry: [
    "src/**/*.ts",
    "!src/**/*.test.ts",
    "!src/**/*.spec.ts",
    "!src/**/*.d.ts",
  ],
  sourcemap: true,
  dts: false,
  deps: {
    onlyBundle: false,
  },
});
