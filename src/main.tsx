import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import "./styles.css";
import "./components/plugin-pages.css";

const container = document.getElementById("root");
if (!container) {
  throw new Error("找不到 #root 容器");
}
createRoot(container).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
