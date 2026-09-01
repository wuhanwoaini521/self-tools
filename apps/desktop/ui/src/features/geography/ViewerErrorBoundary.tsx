import { Component, type ReactNode } from "react";

interface ViewerErrorBoundaryProps {
  children: ReactNode;
  /** 发生错误时要展示的回退内容；缺省时显示内置的降级提示。 */
  fallback?: ReactNode;
}

interface ViewerErrorBoundaryState {
  hasError: boolean;
}

/**
 * 把高风险的视频/地图类子组件（WebGL、网络瓦片、懒加载模块）隔离在错误边界内：
 * 即使它内部在渲染或模块加载阶段抛错，也只降级为可用的提示，绝不卸载整个应用
 * （本项目没有全局错误边界，一次未捕获的运行时错误会连根拔掉全部样式）。
 */
export class ViewerErrorBoundary extends Component<
  ViewerErrorBoundaryProps,
  ViewerErrorBoundaryState
> {
  state: ViewerErrorBoundaryState = { hasError: false };

  static getDerivedStateFromError(): ViewerErrorBoundaryState {
    return { hasError: true };
  }

  componentDidCatch(error: unknown) {
    console.error("[viewer-error-boundary] viewer crashed, degraded:", error);
  }

  render() {
    if (this.state.hasError) {
      if (this.props.fallback) return this.props.fallback;
      return (
        <div className="realterrain-error" role="alert">
          <span>真实地形模块暂时无法显示。</span>
          <p>可切换回「概念地形」继续学习；其他功能不受影响。</p>
        </div>
      );
    }
    return this.props.children;
  }
}

export default ViewerErrorBoundary;
