import { invoke } from "@tauri-apps/api/core";
import { Database, X } from "@phosphor-icons/react";
import { useCallback, useEffect, useState } from "react";
import type {
  GeographySettings,
  SourceInfo,
  StarterReport,
  TravelSearchBackend,
  TravelSettings,
} from "./types";
import { loadAmap } from "./features/geography/AmapMap";
import { allThemes, getTheme } from "./theme/ThemeManager";
import { errorMessage, isTauriRuntime } from "./utils";

interface SettingsDialogProps {
  themeId: string;
  onThemeChange: (themeId: string) => void;
  rssRefreshMinutes: number;
  onRefreshMinutesChange: (minutes: number) => void;
  travel: TravelSettings;
  onTravelChange: (travel: TravelSettings) => void;
  geography: GeographySettings;
  onGeographyChange: (geography: GeographySettings) => void;
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
export function SettingsDialog({
  themeId,
  onThemeChange,
  rssRefreshMinutes,
  onRefreshMinutesChange,
  travel,
  onTravelChange,
  geography,
  onGeographyChange,
  onClose,
}: SettingsDialogProps) {
  const current = getTheme(themeId);
  const [testing, setTesting] = useState<
    "llm" | "amap" | "geography-amap" | "qweather" | null
  >(null);
  const [testResults, setTestResults] = useState<
    Partial<Record<"llm" | "amap" | "geography-amap" | "qweather", string>>
  >({});
  const updateTravel = (patch: Partial<TravelSettings>) =>
    onTravelChange({ ...travel, ...patch });
  const updateGeography = (patch: Partial<GeographySettings>) =>
    onGeographyChange({ ...geography, ...patch });
  const runTest = async (
    kind: "llm" | "amap" | "geography-amap" | "qweather",
  ) => {
    if (!isTauriRuntime()) return;
    setTesting(kind);
    setTestResults((current) => ({ ...current, [kind]: "" }));
    try {
      const result =
        kind === "llm"
          ? await invoke<string>("test_travel_llm", {
              request: {
                baseUrl: travel.llm_base_url ?? "",
                apiKey: travel.llm_api_key,
                model: travel.llm_model ?? "",
              },
            })
          : kind === "geography-amap"
            ? await loadAmap(
                geography.amap_api_key ?? "",
                geography.amap_security_js_code ?? "",
              ).then(() => "Geography 高德 JS API 加载成功")
            : kind === "amap"
              ? await invoke<string>("test_travel_amap", {
                  request: { apiKey: travel.amap_api_key ?? "", apiHost: null },
                })
              : await invoke<string>("test_travel_qweather", {
                  request: {
                    apiKey: travel.qweather_api_key ?? "",
                    apiHost: travel.qweather_api_host,
                  },
                });
      setTestResults((current) => ({ ...current, [kind]: result }));
    } catch (error) {
      setTestResults((current) => ({
        ...current,
        [kind]: `连接失败：${errorMessage(error)}`,
      }));
    } finally {
      setTesting(null);
    }
  };
  return (
    <div className="settings-backdrop" onMouseDown={onClose}>
      <section
        className="settings-dialog"
        onMouseDown={(event) => event.stopPropagation()}
        aria-label="设置"
      >
        <header>
          <div>
            <h2>设置</h2>
            <p className="settings-subtitle">
              个性化 DevToolbox 的外观与行为。
            </p>
          </div>
          <button title="关闭设置" onClick={onClose}>
            <X size={16} />
          </button>
        </header>
        <div className="settings-body">
          <section className="settings-section">
            <label className="settings-label" htmlFor="ui-theme-select">
              界面风格
            </label>
            <p className="settings-hint">
              切换立即生效,无需重启;下次启动自动恢复。
            </p>
            <select
              className="settings-select"
              id="ui-theme-select"
              value={current.id}
              onChange={(event) => onThemeChange(event.target.value)}
            >
              {allThemes().map((theme) => (
                <option key={theme.id} value={theme.id}>
                  {theme.name}
                </option>
              ))}
            </select>
            <p className="settings-theme-description">{current.description}</p>
          </section>
          <section className="settings-section">
            <label className="settings-label">Geography · 高德地图</label>
            <p className="settings-hint">
              与 Travel 独立保存。填写高德「Web 端（JS API）」Key
              和对应安全密钥后，地理探索页使用真实动态地图；留空时使用离线地图。
            </p>
            <input
              className="settings-select"
              type="password"
              placeholder="高德 AMAP_JS_API_KEY（Geography 专用）"
              value={geography.amap_api_key ?? ""}
              onChange={(event) =>
                updateGeography({ amap_api_key: event.target.value || null })
              }
              style={{ marginTop: 6 }}
            />
            <input
              className="settings-select"
              type="password"
              placeholder="高德 securityJsCode（Geography 专用）"
              value={geography.amap_security_js_code ?? ""}
              onChange={(event) =>
                updateGeography({
                  amap_security_js_code: event.target.value || null,
                })
              }
              style={{ marginTop: 6 }}
            />
            <button
              className="settings-test-button"
              disabled={
                testing !== null ||
                !geography.amap_api_key?.trim() ||
                !geography.amap_security_js_code?.trim()
              }
              onClick={() => void runTest("geography-amap")}
            >
              {testing === "geography-amap"
                ? "正在测试…"
                : "测试 Geography 高德连接"}
            </button>
            {testResults["geography-amap"] ? (
              <p
                className={
                  testResults["geography-amap"]?.startsWith("连接失败")
                    ? "settings-test-result failed"
                    : "settings-test-result"
                }
              >
                {testResults["geography-amap"]}
              </p>
            ) : null}
            <p className="settings-hint" style={{ marginTop: 6 }}>
              测试会加载高德 JS API；成功后切换到 Geography
              页面即可看到动态地图。
            </p>
          </section>
          <section className="settings-section">
            <label className="settings-label" htmlFor="rss-refresh-select">
              RSS 刷新频率
            </label>
            <p className="settings-hint">后台按此间隔自动拉取订阅更新。</p>
            <select
              className="settings-select"
              id="rss-refresh-select"
              value={rssRefreshMinutes}
              onChange={(event) =>
                onRefreshMinutesChange(Number(event.target.value))
              }
            >
              {REFRESH_CHOICES.map((minutes) => (
                <option key={minutes} value={minutes}>
                  每 {minutes} 分钟
                </option>
              ))}
            </select>
          </section>
          <section className="settings-section">
            <label className="settings-label" htmlFor="travel-backend-select">
              Travel · 搜索后端
            </label>
            <p className="settings-hint">
              优先使用国内可访问的搜索引擎；SearXNG 未配置 / 不可用时自动回退
              Bing 中国。海外搜索仅作可选的后续扩展，不作为必需依赖。
            </p>
            <select
              className="settings-select"
              id="travel-backend-select"
              value={travel.search_backend}
              onChange={(event) =>
                updateTravel({
                  search_backend: event.target.value as TravelSearchBackend,
                })
              }
            >
              {SEARCH_BACKENDS.map((backend) => (
                <option key={backend.value} value={backend.value}>
                  {backend.label}
                </option>
              ))}
            </select>
            {travel.search_backend === "searxng" ? (
              <>
                <label
                  className="settings-label"
                  htmlFor="travel-searxng-url"
                  style={{ marginTop: 10 }}
                >
                  SearXNG 地址
                </label>
                <input
                  className="settings-select"
                  id="travel-searxng-url"
                  type="text"
                  placeholder="http://localhost:8080"
                  value={travel.searxng_url ?? ""}
                  onChange={(event) =>
                    updateTravel({ searxng_url: event.target.value || null })
                  }
                />
              </>
            ) : null}
          </section>
          <section className="settings-section">
            <label className="settings-label" htmlFor="travel-llm-base">
              Travel · LLM（OpenAI Compatible）
            </label>
            <p className="settings-hint">
              用于补充搜索主题、提取事实与生成结构化攻略。支持 DeepSeek / Qwen /
              OpenAI / 本地 Ollama（http://localhost:11434/v1）。留空时 Travel
              以“来源列表”模式运行。
            </p>
            <input
              className="settings-select"
              id="travel-llm-base"
              type="text"
              placeholder="https://api.deepseek.com/v1"
              value={travel.llm_base_url ?? ""}
              onChange={(event) =>
                updateTravel({ llm_base_url: event.target.value || null })
              }
            />
            <label
              className="settings-label"
              htmlFor="travel-llm-model"
              style={{ marginTop: 10 }}
            >
              模型
            </label>
            <input
              className="settings-select"
              id="travel-llm-model"
              type="text"
              placeholder="deepseek-chat / qwen-plus / qwen2.5:7b"
              value={travel.llm_model ?? ""}
              onChange={(event) =>
                updateTravel({ llm_model: event.target.value || null })
              }
            />
            <label
              className="settings-label"
              htmlFor="travel-llm-key"
              style={{ marginTop: 10 }}
            >
              API Key（本地 Ollama 可留空）
            </label>
            <input
              className="settings-select"
              id="travel-llm-key"
              type="password"
              placeholder="sk-..."
              value={travel.llm_api_key ?? ""}
              onChange={(event) =>
                updateTravel({ llm_api_key: event.target.value || null })
              }
            />
            <button
              className="settings-test-button"
              disabled={
                testing !== null ||
                !travel.llm_base_url?.trim() ||
                !travel.llm_model?.trim()
              }
              onClick={() => void runTest("llm")}
            >
              {testing === "llm" ? "正在测试…" : "测试 LLM 连接"}
            </button>
            {testResults.llm ? (
              <p
                className={
                  testResults.llm.startsWith("连接失败")
                    ? "settings-test-result failed"
                    : "settings-test-result"
                }
              >
                {testResults.llm}
              </p>
            ) : null}
          </section>
          <section className="settings-section">
            <label className="settings-label">Travel · 可选数据源 Key</label>
            <p className="settings-hint">
              高德 POI
              与和风天气已接入；百度地图预留。和风天气请填写控制台“设置”中的专属
              API Host（不再使用 devapi.qweather.com）。全部可选：不填时 Travel
              核心功能不受影响。
            </p>
            <input
              className="settings-select"
              type="password"
              placeholder="高德 AMAP_API_KEY（Web服务类型）"
              value={travel.amap_api_key ?? ""}
              onChange={(event) =>
                updateTravel({ amap_api_key: event.target.value || null })
              }
              style={{ marginTop: 6 }}
            />
            <button
              className="settings-test-button"
              disabled={testing !== null || !travel.amap_api_key?.trim()}
              onClick={() => void runTest("amap")}
            >
              {testing === "amap" ? "正在测试…" : "测试高德连接"}
            </button>
            {testResults.amap ? (
              <p
                className={
                  testResults.amap.startsWith("连接失败")
                    ? "settings-test-result failed"
                    : "settings-test-result"
                }
              >
                {testResults.amap}
              </p>
            ) : null}
            <input
              className="settings-select"
              type="text"
              placeholder="和风天气 API Host，例如 abc123.qweatherapi.com"
              value={travel.qweather_api_host ?? ""}
              onChange={(event) =>
                updateTravel({ qweather_api_host: event.target.value || null })
              }
              style={{ marginTop: 10 }}
            />
            <input
              className="settings-select"
              type="password"
              placeholder="和风天气 QWEATHER_API_KEY（可选）"
              value={travel.qweather_api_key ?? ""}
              onChange={(event) =>
                updateTravel({ qweather_api_key: event.target.value || null })
              }
              style={{ marginTop: 6 }}
            />
            <button
              className="settings-test-button"
              disabled={
                testing !== null ||
                !travel.qweather_api_key?.trim() ||
                !travel.qweather_api_host?.trim()
              }
              onClick={() => void runTest("qweather")}
            >
              {testing === "qweather" ? "正在测试…" : "测试和风天气连接"}
            </button>
            <input
              className="settings-select"
              type="password"
              placeholder="百度地图 BAIDU_MAP_API_KEY（预留）"
              value={travel.baidu_map_api_key ?? ""}
              onChange={(event) =>
                updateTravel({ baidu_map_api_key: event.target.value || null })
              }
              style={{ marginTop: 6 }}
            />
            {testResults.qweather ? (
              <p
                className={
                  testResults.qweather.startsWith("连接失败")
                    ? "settings-test-result failed"
                    : "settings-test-result"
                }
              >
                {testResults.qweather}
              </p>
            ) : null}
          </section>
          <section className="settings-section">
            <label className="settings-label">Language · 语言数据（#90）</label>
            <p className="settings-hint">
              所有词条均可追溯来源；许可能力位（署名/商用/再分发/相同方式共享）在下方展示。内置
              Starter Pack 为真实数据子集，完整数据包用{" "}
              <code>language-data import …</code>{" "}
              导入（docs/language/DATA_SOURCES.md）。
            </p>
            <LanguageDataSection />
          </section>
        </div>
      </section>
    </div>
  );
}

/** Language Data（#90）：来源/版本/许可/导入条目数 + 安装内置 Starter Pack。 */
function LanguageDataSection() {
  const [sources, setSources] = useState<SourceInfo[]>([]);
  const [installNotice, setInstallNotice] = useState("");
  const [installing, setInstalling] = useState(false);

  const reload = useCallback(async () => {
    if (!isTauriRuntime()) return;
    try {
      setSources(await invoke<SourceInfo[]>("language_sources"));
    } catch {
      // 设置面板静默失败
    }
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

  const install = async () => {
    if (!isTauriRuntime() || installing) return;
    setInstalling(true);
    setInstallNotice("");
    try {
      const report = await invoke<StarterReport>("language_install_starter", {
        only: null,
      });
      setInstallNotice(
        `已安装：+${report.total_inserted} 条，更新 ${report.total_updated} 条`,
      );
      void reload();
    } catch (error) {
      setInstallNotice(`安装失败：${errorMessage(error)}`);
    } finally {
      setInstalling(false);
    }
  };

  return (
    <div className="lang-sources-settings">
      <button
        className="settings-test-button"
        onClick={() => void install()}
        disabled={installing}
      >
        <Database size={14} />
        {installing ? "安装中…" : "安装 Starter Pack（内置真实数据子集）"}
      </button>
      {installNotice ? (
        <p className="settings-test-result">{installNotice}</p>
      ) : null}
      {sources.length === 0 ? (
        <p className="settings-hint">
          尚未导入数据源（可点击上方按钮安装内置包）。
        </p>
      ) : (
        <ul className="lang-sources-list">
          {sources.map((item) => (
            <li key={item.source.id}>
              <div className="lang-source-row">
                <b>{item.source.name}</b>
                <span>
                  {item.source.license.kind}
                  {item.source.license.attribution_required ? " · 需署名" : ""}
                </span>
                <small>
                  {item.source.dataset_version} · {item.item_count} 条
                </small>
              </div>
              <p className="lang-muted">
                {item.source.attribution || item.source.homepage}
              </p>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
