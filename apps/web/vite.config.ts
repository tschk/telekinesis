import path from "node:path";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

// https://vite.dev/config/
export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      "@tk/workspace-stream": path.resolve(__dirname, "../stream/workspaceStream.ts"),
    },
  },
});
