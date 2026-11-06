import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tauriConfig from "./src-tauri/tauri.conf.json";

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: { port: 1420, strictPort: true, host: "127.0.0.1" },
  preview: {
    port: 1420,
    strictPort: true,
    host: "127.0.0.1",
    headers: { "Content-Security-Policy": tauriConfig.app.security.csp },
  },
  envPrefix: ["VITE_", "TAURI_ENV_"],
  build: { target: ["es2022", "safari13"] },
});
