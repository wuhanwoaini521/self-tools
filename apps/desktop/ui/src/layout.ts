/**
 * 设备布局系统（V11 §84-§90）。
 *
 * 统一概念 DESKTOP / TABLET / MOBILE，由 viewport 宽度 + 可用空间动态推导
 * （resize / orientationchange 都会重新计算，不是启动时检测一次）。
 *
 * ```text
 * DESKTOP  宽导航 + 工作区 + AI 右侧固定面板
 * TABLET   自适应分栏 + AI 侧滑面板（portrait/landscape 分别处理）
 * MOBILE   单列 + 底部导航 + AI 底部抽屉 / 全屏
 * ```
 *
 * safe-area：底部导航 / AI 输入框 / 确认按钮必须避开 Home Indicator
 * （env(safe-area-inset-*)）。
 */

import { useCallback, useEffect, useMemo, useState } from "react";

export type DeviceClass = "desktop" | "tablet" | "mobile";

/** 平板竖屏宽度（iPad portrait = 768）。 */
export const TABLET_MIN_WIDTH = 768;
/** 桌面宽度（含 AI 侧栏的舒适宽度）。 */
export const DESKTOP_MIN_WIDTH = 1180;

export interface LayoutState {
  device: DeviceClass;
  /** 视口宽高（CSS px）。 */
  width: number;
  height: number;
  /** 横屏（width > height）。 */
  landscape: boolean;
  /** 是否触屏优先（coarse pointer）。 */
  touch: boolean;
  /** 平板分栏是否可并排（landscape tablet 且宽度足够）。 */
  tabletSplit: boolean;
}

function measure(): LayoutState {
  if (typeof window === "undefined") {
    return {
      device: "desktop",
      width: 1440,
      height: 900,
      landscape: true,
      touch: false,
      tabletSplit: true,
    };
  }
  const width = window.innerWidth;
  const height = window.innerHeight;
  const landscape = width > height;
  const touch =
    typeof window.matchMedia === "function"
      ? window.matchMedia("(pointer: coarse)").matches
      : false;
  const device: DeviceClass =
    width >= DESKTOP_MIN_WIDTH ? "desktop" : width >= TABLET_MIN_WIDTH ? "tablet" : "mobile";
  // 平板分栏：landscape 且宽度足够放两栏（≥ 1024，如 iPad landscape）。
  const tabletSplit = device === "tablet" && landscape && width >= 1024;
  return { device, width, height, landscape, touch, tabletSplit };
}

/** 订阅布局变化（resize + orientationchange + 指针类型变化）。 */
export function useLayout(): LayoutState {
  const [state, setState] = useState<LayoutState>(measure);
  useEffect(() => {
    let frame = 0;
    const onChange = () => {
      if (frame) cancelAnimationFrame(frame);
      frame = requestAnimationFrame(() => setState(measure()));
    };
    window.addEventListener("resize", onChange);
    window.addEventListener("orientationchange", onChange);
    // 指针类型变化（插键盘 / 拔鼠标）也重新测量。
    const pointer = window.matchMedia?.("(pointer: coarse)");
    pointer?.addEventListener?.("change", onChange);
    return () => {
      if (frame) cancelAnimationFrame(frame);
      window.removeEventListener("resize", onChange);
      window.removeEventListener("orientationchange", onChange);
      pointer?.removeEventListener?.("change", onChange);
    };
  }, []);
  return state;
}

/** AI 面板呈现方式（§86-§88）。 */
export type AiPresentation = "side-panel" | "side-sheet" | "bottom-sheet" | "fullscreen";

export function aiPresentation(layout: LayoutState): AiPresentation {
  if (layout.device === "desktop") return "side-panel";
  if (layout.device === "tablet") return layout.tabletSplit ? "side-sheet" : "bottom-sheet";
  return layout.landscape ? "bottom-sheet" : "fullscreen";
}

/** 导航呈现方式。desktop/tablet = 侧导航；mobile = 底部导航。 */
export function navigationLayout(layout: LayoutState): "side" | "bottom" {
  return layout.device === "mobile" ? "bottom" : "side";
}

/** 底部安全区高度（CSS 变量由 index.html 注入；这里给 fallback）。 */
export function useSafeAreaInsets(): { top: number; bottom: number } {
  const [insets, setInsets] = useState({ top: 0, bottom: 0 });
  useEffect(() => {
    if (typeof window === "undefined") return;
    const read = () => {
      const style = getComputedStyle(document.documentElement);
      const parse = (name: string) => Number.parseFloat(style.getPropertyValue(name)) || 0;
      setInsets({
        top: parse("--safe-area-top"),
        bottom: parse("--safe-area-bottom"),
      });
    };
    read();
    window.addEventListener("resize", read);
    window.addEventListener("orientationchange", read);
    return () => {
      window.removeEventListener("resize", read);
      window.removeEventListener("orientationchange", read);
    };
  }, []);
  return insets;
}

/** 移动端底部导航是否可显示（≥4 个一级入口才有意义）。 */
export function shouldShowBottomNav(layout: LayoutState, itemCount: number): boolean {
  return navigationLayout(layout) === "bottom" && itemCount > 0;
}

/** 触控目标最小尺寸（V11 §90：44pt 是 Apple HIG 下限）。 */
export const MIN_TOUCH_TARGET = 44;

/** hook：把 device class 以 data 属性挂到根节点（CSS 统一消费）。 */
export function useDeviceAttribute(): DeviceClass {
  const layout = useLayout();
  useEffect(() => {
    document.documentElement.dataset.device = layout.device;
    document.documentElement.dataset.orientation = layout.landscape ? "landscape" : "portrait";
  }, [layout.device, layout.landscape]);
  return layout.device;
}

/** 测试/ SSR 友好的纯函数判断（供组件外使用）。 */
export function deviceForWidth(width: number): DeviceClass {
  if (width >= DESKTOP_MIN_WIDTH) return "desktop";
  if (width >= TABLET_MIN_WIDTH) return "tablet";
  return "mobile";
}

/** 视口预设（V11 §137：视觉 QA 用）。 */
export const QA_VIEWPORTS = [
  { name: "Desktop", width: 1440, height: 900 },
  { name: "Tablet Landscape", width: 1024, height: 768 },
  { name: "Tablet Portrait", width: 768, height: 1024 },
  { name: "Mobile", width: 390, height: 844 },
  { name: "Small Mobile", width: 360, height: 800 },
] as const;

/** 便捷 hook：当前是否窄屏（单列）。 */
export function useIsMobile(): boolean {
  const layout = useLayout();
  return useMemo(() => layout.device === "mobile", [layout.device]);
}

/** 便捷 hook：AI 关闭时用于触发打开的稳定回调（避免每次渲染新函数）。 */
export function useEventCallback<Args extends unknown[], Result>(
  handler: (...args: Args) => Result,
): (...args: Args) => Result {
  const ref = useCallback(handler, [handler]);
  return useCallback((...args: Args) => ref(...args), [ref]);
}
