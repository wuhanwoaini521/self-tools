import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import { applyTheme, initialThemeId } from "./theme/ThemeManager";
import "./theme/themes";
import "./styles.css";

// 首帧渲染前同步恢复主题快照,避免启动时先闪 Default 再切换的闪烁。
applyTheme(initialThemeId());

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
