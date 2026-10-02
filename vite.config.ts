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
    // `artifacts/` 是 ui-check 的产物目录（每次运行写入约 80 张截图）。它必须排除在监视
    // 之外：否则每张新截图都要占一个 watcher，跑几次就能把用户级的 inotify 额度耗尽，
    // 让开发服务器以 ENOSPC 崩掉。
    watch: { ignored: ["**/src-tauri/**", "**/artifacts/**"] },
  },
}));
