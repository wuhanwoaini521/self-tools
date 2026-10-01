import { Component, StrictMode, type ErrorInfo, type ReactNode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import { applyTheme, initialThemeId } from "./theme/ThemeManager";
import { registerPwa } from "./pwa";
import "./theme/themes";
import "./styles.css";

// WebDriver hooks exist only in the dedicated E2E bundle. The production UI
// neither imports the test bridge nor exposes its Tauri plugin surface.
if (import.meta.env.VITE_TAURI_E2E === "1") {
  window.__DEVTOOLBOX_E2E__ = true;
  document.documentElement.dataset.e2e = "true";
  // The Tauri service correlates native and WebDriver windows by title.
  document.title = "DevToolbox";
}

interface StartupErrorBoundaryState {
  error: Error | null;
  componentStack: string;
}

class StartupErrorBoundary extends Component<
  { children: ReactNode },
  StartupErrorBoundaryState
> {
  state: StartupErrorBoundaryState = { error: null, componentStack: "" };

  static getDerivedStateFromError(error: Error): StartupErrorBoundaryState {
    return { error, componentStack: "" };
  }

  componentDidCatch(error: Error, info: ErrorInfo): void {
    console.error("[startup] React render failed", error, info.componentStack);
    this.setState({ componentStack: info.componentStack ?? "" });
  }

  render() {
    if (this.state.error) {
      return (
        <main
          style={{
            minHeight: "100vh",
            padding: "48px",
            color: "#1e1e1b",
            background: "#f7f5ef",
            fontFamily: '"Segoe UI", sans-serif',
          }}
        >
          <h1>DevToolbox 启动失败</h1>
          <p>界面加载时发生了前端异常，请查看下面的错误信息：</p>
          <pre style={{ whiteSpace: "pre-wrap", color: "#e11d48", fontWeight: "bold", fontSize: 15 }}>
            {this.state.error.name}: {this.state.error.message}
          </pre>
          {this.state.error.stack ? (
            <pre style={{ whiteSpace: "pre-wrap", color: "#6b7280", fontSize: 12 }}>
              {this.state.error.stack}
            </pre>
          ) : null}
          {this.state.componentStack ? (
            <pre style={{ whiteSpace: "pre-wrap", color: "#374151", fontSize: 12 }}>
              {this.state.componentStack}
            </pre>
          ) : null}
          <button type="button" onClick={() => window.location.reload()}>
            重新加载
          </button>
        </main>
      );
    }
    return this.props.children;
  }
}

// 首帧渲染前同步恢复主题快照,避免启动时先闪 Default 再切换的闪烁。
applyTheme(initialThemeId());

// PWA（V11 §76-§82）：注册 service worker + 更新/离线事件。
registerPwa({
  onStateChange: (state) => {
    document.documentElement.dataset.pwa = state.status;
    if (!state.online) document.documentElement.dataset.offline = "true";
    else delete document.documentElement.dataset.offline;
  },
});

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <StartupErrorBoundary>
      <App />
    </StartupErrorBoundary>
  </StrictMode>,
);
