import { defineConfig, loadEnv } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig(({ mode }) => {
  // The Rust API the dev server proxies to; e2e runs point it at a fixture server.
  const apiTarget = loadEnv(mode, ".", "MIKU_").MIKU_API_URL || "http://127.0.0.1:3000";
  return {
    plugins: [react(), tailwindcss()],
    server: {
      host: "0.0.0.0",
      port: 5173,
      strictPort: true,
      allowedHosts: ["harus-macmini"],
      proxy: {
        "/api": apiTarget,
        "/events": apiTarget
      }
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
          }
        }
      }
    }
  };
});
