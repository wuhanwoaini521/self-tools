export interface AmapOverlay {
  on: (event: string, handler: () => void) => void;
}

export interface AmapMapInstance {
  add: (overlays: AmapOverlay | AmapOverlay[]) => void;
  remove: (overlays: AmapOverlay | AmapOverlay[]) => void;
  clearMap: () => void;
  setCenter?: (center: [number, number]) => void;
  setFitView: (overlays?: AmapOverlay[], immediately?: boolean, avoid?: number[], maxZoom?: number) => void;
  destroy: () => void;
}

export interface AmapDistrict {
  name?: string;
  center?: [number, number] | string;
  boundaries?: Array<Array<[number, number]>>;
}

export interface AmapDistrictSearchResult {
  districtList?: AmapDistrict[];
}

export interface AmapDistrictSearch {
  search: (keyword: string, callback: (status: string, result: AmapDistrictSearchResult) => void) => void;
}

export interface AmapNamespace {
  Map: new (container: HTMLDivElement, options: Record<string, unknown>) => AmapMapInstance;
  Marker: new (options: Record<string, unknown>) => AmapOverlay;
  Polyline: new (options: Record<string, unknown>) => AmapOverlay;
  Polygon: new (options: Record<string, unknown>) => AmapOverlay;
  DistrictSearch?: new (options: Record<string, unknown>) => AmapDistrictSearch;
  plugin?: (plugins: string[], callback: () => void) => void;
}

declare global {
  interface Window {
    AMap?: AmapNamespace;
    _AMapSecurityConfig?: { securityJsCode?: string };
  }
}

const AMAP_SCRIPT_ID = "devtoolbox-amap-js-api";

export function loadAmap(apiKey: string, securityJsCode: string) {
  if (!apiKey.trim()) return Promise.reject(new Error("请先填写 Geography 的高德 JS API Key"));
  if (!securityJsCode.trim()) return Promise.reject(new Error("请先填写 Geography 的 securityJsCode"));
  if (window.AMap) return Promise.resolve(window.AMap);
  return new Promise<AmapNamespace>((resolve, reject) => {
    const callbackName = `__devtoolbox_amap_ready_${Date.now()}`;
    const globalWindow = window as unknown as Window & Record<string, unknown>;
    globalWindow._AMapSecurityConfig = { securityJsCode };
    globalWindow[callbackName] = () => {
      delete globalWindow[callbackName];
      if (window.AMap) resolve(window.AMap);
      else reject(new Error("高德 JS API 已加载，但未找到 AMap 对象"));
    };
    const script = document.createElement("script");
    script.id = AMAP_SCRIPT_ID;
    script.async = true;
    script.src = `https://webapi.amap.com/maps?v=2.0&key=${encodeURIComponent(apiKey)}&callback=${callbackName}`;
    script.onerror = () => {
      delete globalWindow[callbackName];
      reject(new Error("高德 JS API 加载失败，请检查 Key、Security Code 或网络连接"));
    };
    document.head.appendChild(script);
  });
}

export function loadAmapPlugin(amap: AmapNamespace, plugins: string[]) {
  if (!amap.plugin) return Promise.reject(new Error("高德 JS API 不支持插件加载"));
  return new Promise<void>((resolve) => amap.plugin?.(plugins, resolve));
}
