import { X } from "@phosphor-icons/react";
import { allThemes, getTheme } from "./theme/ThemeManager";

interface SettingsDialogProps {
  themeId: string;
  onThemeChange: (themeId: string) => void;
  rssRefreshMinutes: number;
  onRefreshMinutesChange: (minutes: number) => void;
  onClose: () => void;
}

const REFRESH_CHOICES = [15, 30, 60];

/**
 * 全局设置(属于整个应用,而非某个 Feature)。
 * 目前包含:Appearance(界面风格)与 RSS(刷新间隔);后续分区按需增加。
 * 主题选项来自 ThemeManager 注册表,新主题注册后自动出现。
 */
export function SettingsDialog({ themeId, onThemeChange, rssRefreshMinutes, onRefreshMinutesChange, onClose }: SettingsDialogProps) {
  const current = getTheme(themeId);
  return <div className="settings-backdrop" onMouseDown={onClose}>
    <section className="settings-dialog" onMouseDown={(event) => event.stopPropagation()} aria-label="设置">
      <header>
        <div>
          <h2>设置</h2>
          <p className="settings-subtitle">个性化 DevToolbox 的外观与行为。</p>
        </div>
        <button title="关闭设置" onClick={onClose}><X size={16} /></button>
      </header>
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
    </section>
  </div>;
}
