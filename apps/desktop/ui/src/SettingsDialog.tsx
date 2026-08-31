import { X } from "@phosphor-icons/react";
import type { TravelSearchBackend, TravelSettings } from "./types";
import { allThemes, getTheme } from "./theme/ThemeManager";

interface SettingsDialogProps {
  themeId: string;
  onThemeChange: (themeId: string) => void;
  rssRefreshMinutes: number;
  onRefreshMinutesChange: (minutes: number) => void;
  travel: TravelSettings;
  onTravelChange: (travel: TravelSettings) => void;
  onClose: () => void;
}

const REFRESH_CHOICES = [15, 30, 60];

const SEARCH_BACKENDS: { value: TravelSearchBackend; label: string }[] = [
  { value: "auto", label: "自动（Bing 中国 → 百度）" },
  { value: "searxng", label: "本地 SearXNG（推荐自托管）" },
  { value: "baidu", label: "百度" },
  { value: "bing", label: "Bing 中国" },
];

/**
 * 全局设置。
 * 目前包含:Appearance(界面风格)、RSS(刷新间隔)与 Travel(搜索后端 / LLM / 可选数据源 Key)。
 * Travel 全部为可选配置：未配置时模块仍可运行（搜索用默认后端、无 LLM 降级为来源列表）。
 */
export function SettingsDialog({ themeId, onThemeChange, rssRefreshMinutes, onRefreshMinutesChange, travel, onTravelChange, onClose }: SettingsDialogProps) {
  const current = getTheme(themeId);
  const updateTravel = (patch: Partial<TravelSettings>) => onTravelChange({ ...travel, ...patch });
  return <div className="settings-backdrop" onMouseDown={onClose}>
    <section className="settings-dialog" onMouseDown={(event) => event.stopPropagation()} aria-label="设置">
      <header>
        <div>
          <h2>设置</h2>
          <p className="settings-subtitle">个性化 DevToolbox 的外观与行为。</p>
        </div>
        <button title="关闭设置" onClick={onClose}><X size={16} /></button>
      </header>
      <div className="settings-body">
      <section className="settings-section">
        <label className="settings-label" htmlFor="ui-theme-select">界面风格</label>
        <p className="settings-hint">切换立即生效,无需重启;下次启动自动恢复。</p>
        <select className="settings-select" id="ui-theme-select" value={current.id}
          onChange={(event) => onThemeChange(event.target.value)}>
          {allThemes().map((theme) => <option key={theme.id} value={theme.id}>{theme.name}</option>)}
        </select>
        <p className="settings-theme-description">{current.description}</p>
      </section>
      <section className="settings-section">
        <label className="settings-label" htmlFor="rss-refresh-select">RSS 刷新频率</label>
        <p className="settings-hint">后台按此间隔自动拉取订阅更新。</p>
        <select className="settings-select" id="rss-refresh-select" value={rssRefreshMinutes}
          onChange={(event) => onRefreshMinutesChange(Number(event.target.value))}>
          {REFRESH_CHOICES.map((minutes) => <option key={minutes} value={minutes}>每 {minutes} 分钟</option>)}
        </select>
      </section>
      <section className="settings-section">
        <label className="settings-label" htmlFor="travel-backend-select">Travel · 搜索后端</label>
        <p className="settings-hint">优先使用国内可访问的搜索引擎；SearXNG 未配置 / 不可用时自动回退 Bing 中国。海外搜索仅作可选的后续扩展，不作为必需依赖。</p>
        <select className="settings-select" id="travel-backend-select" value={travel.search_backend}
          onChange={(event) => updateTravel({ search_backend: event.target.value as TravelSearchBackend })}>
          {SEARCH_BACKENDS.map((backend) => <option key={backend.value} value={backend.value}>{backend.label}</option>)}
        </select>
        {travel.search_backend === "searxng" ? <>
          <label className="settings-label" htmlFor="travel-searxng-url" style={{ marginTop: 10 }}>SearXNG 地址</label>
          <input className="settings-select" id="travel-searxng-url" type="text" placeholder="http://localhost:8080"
            value={travel.searxng_url ?? ""} onChange={(event) => updateTravel({ searxng_url: event.target.value || null })} />
        </> : null}
      </section>
      <section className="settings-section">
        <label className="settings-label" htmlFor="travel-llm-base">Travel · LLM（OpenAI Compatible）</label>
        <p className="settings-hint">用于补充搜索主题、提取事实与生成结构化攻略。支持 DeepSeek / Qwen / OpenAI / 本地 Ollama（http://localhost:11434/v1）。留空时 Travel 以“来源列表”模式运行。</p>
        <input className="settings-select" id="travel-llm-base" type="text" placeholder="https://api.deepseek.com/v1"
          value={travel.llm_base_url ?? ""} onChange={(event) => updateTravel({ llm_base_url: event.target.value || null })} />
        <label className="settings-label" htmlFor="travel-llm-model" style={{ marginTop: 10 }}>模型</label>
        <input className="settings-select" id="travel-llm-model" type="text" placeholder="deepseek-chat / qwen-plus / qwen2.5:7b"
          value={travel.llm_model ?? ""} onChange={(event) => updateTravel({ llm_model: event.target.value || null })} />
        <label className="settings-label" htmlFor="travel-llm-key" style={{ marginTop: 10 }}>API Key（本地 Ollama 可留空）</label>
        <input className="settings-select" id="travel-llm-key" type="password" placeholder="sk-..."
          value={travel.llm_api_key ?? ""} onChange={(event) => updateTravel({ llm_api_key: event.target.value || null })} />
      </section>
      <section className="settings-section">
        <label className="settings-label">Travel · 可选数据源 Key</label>
        <p className="settings-hint">高德 POI 与和风天气已接入；百度地图预留。全部可选：不填时 Travel 核心功能不受影响，研究时该步骤会标记为跳过。</p>
        <input className="settings-select" type="password" placeholder="高德 AMAP_API_KEY（Web服务类型）" value={travel.amap_api_key ?? ""} onChange={(event) => updateTravel({ amap_api_key: event.target.value || null })} style={{ marginTop: 6 }} />
        <input className="settings-select" type="password" placeholder="和风天气 QWEATHER_API_KEY（可选）" value={travel.qweather_api_key ?? ""} onChange={(event) => updateTravel({ qweather_api_key: event.target.value || null })} style={{ marginTop: 6 }} />
        <input className="settings-select" type="password" placeholder="百度地图 BAIDU_MAP_API_KEY（预留）" value={travel.baidu_map_api_key ?? ""} onChange={(event) => updateTravel({ baidu_map_api_key: event.target.value || null })} style={{ marginTop: 6 }} />
      </section>
      </div>
    </section>
  </div>;
}