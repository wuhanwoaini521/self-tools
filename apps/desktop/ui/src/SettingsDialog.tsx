import { X } from "@phosphor-icons/react";
import { allThemes, getTheme } from "./theme/ThemeManager";

interface SettingsDialogProps {
  themeId: string;
  onThemeChange: (themeId: string) => void;
  onClose: () => void;
}

/**
 * 设置弹层:目前仅「界面风格」一项 Appearance 设置。
 * 主题选项来自 ThemeManager 注册表,新主题注册后自动出现,无需改动本组件。
 */
export function SettingsDialog({ themeId, onThemeChange, onClose }: SettingsDialogProps) {
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
    </section>
  </div>;
}
