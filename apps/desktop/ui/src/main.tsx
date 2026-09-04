import { Component, StrictMode, type ErrorInfo, type ReactNode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import { applyTheme, initialThemeId } from "./theme/ThemeManager";
import "./theme/themes";
import "./styles.css";

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
            color: "#d6dde1",
            background: "#0d1315",
            fontFamily: '"Segoe UI", sans-serif',
          }}
        >
          <h1>DevToolbox 启动失败</h1>
          <p>界面加载时发生了前端异常，请查看下面的错误信息：</p>
          <pre style={{ whiteSpace: "pre-wrap", color: "#ff9b9b" }}>
            {this.state.error.stack ?? this.state.error.message}
          </pre>
          {this.state.componentStack ? (
            <pre style={{ whiteSpace: "pre-wrap", color: "#aeb8be" }}>
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

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <StartupErrorBoundary>
      <App />
    </StartupErrorBoundary>
  </StrictMode>,
);
