import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import process from "node:process";

const host = process.env.TAURI_DEV_HOST;

// Tauri 期望固定端口，并在该端口不可用时直接失败，而不是静默换端口。
export default defineConfig(() => ({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1421 } : undefined,
    // Rust workspace 的编译产物在根目录 `target/`，不在 `src-tauri/` 内。
    // 将它和 ui-check 的 `artifacts/` 排除，避免大量产物耗尽 inotify 额度而报 ENOSPC。
    watch: { ignored: ["**/src-tauri/**", "**/target/**", "**/artifacts/**"] },
  },
}));
