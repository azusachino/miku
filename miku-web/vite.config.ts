import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  server: {
    host: "0.0.0.0",
    port: 5173,
    strictPort: true,
    allowedHosts: ["harus-macmini"],
    proxy: {
      "/api": "http://127.0.0.1:3000",
      "/events": "http://127.0.0.1:3000",
    },
  },
  build: {
    chunkSizeWarningLimit: 4000,
    rollupOptions: {
      output: {
        manualChunks(id) {
          if (id.includes("node_modules/mermaid") || id.includes("node_modules/cytoscape")) {
            return "vendor-diagrams";
          }
          if (id.includes("node_modules/katex")) {
            return "vendor-katex";
          }
        },
      },
    },
  },


});
