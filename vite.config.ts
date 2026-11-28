import { defineConfig } from "vite";

export default defineConfig({
  clearScreen: false,
  build: {
    rollupOptions: {
      output: {
        manualChunks(id) {
          if (id.includes("/node_modules/echarts/")) return "echarts";
          if (id.includes("/node_modules/zrender/")) return "zrender";
        },
      },
    },
  },
  server: { host: "127.0.0.1", port: 1420, strictPort: true },
});
