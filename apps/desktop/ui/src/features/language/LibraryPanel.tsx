import { invoke } from "@tauri-apps/api/core";
import { BookOpen, Heart, Star } from "@phosphor-icons/react";
import { useCallback, useEffect, useState } from "react";
import type { LanguageCode, LanguageItem, ProgressView, SourceInfo } from "../../types";
import { errorMessage, isTauriRuntime } from "../../utils";
import type { OpenDetail } from "./ExplorePanel";
import { stateLabel } from "./WordDetail";

interface LibraryPanelProps {
  language: LanguageCode;
  onOpen: OpenDetail;
  onUpdated: () => void;
  setNotice: (message: string) => void;
}

/** Library：收藏 / 学习进度 / 数据来源（Settings → Language Data 也在其中展示）。 */
export function LibraryPanel({ language, onOpen, onUpdated, setNotice }: LibraryPanelProps) {
  const [favorites, setFavorites] = useState<LanguageItem[]>([]);
  const [progress, setProgress] = useState<ProgressView | null>(null);
  const [sources, setSources] = useState<SourceInfo[]>([]);

  const reload = useCallback(async () => {
    if (!isTauriRuntime()) return;
    try {
      const [favs, prog, srcs] = await Promise.all([
        invoke<LanguageItem[]>("language_favorites", { limit: 200 }),
        invoke<ProgressView>("language_progress"),
        invoke<SourceInfo[]>("language_sources"),
      ]);
      setFavorites(favs);
      setProgress(prog);
      setSources(srcs);
    } catch (error) {
      setNotice(errorMessage(error));
    }
  }, [setNotice]);

  useEffect(() => { void reload(); }, [reload, onUpdated]);

  return <div className="lang-panel lang-library">
    <section className="lang-card">
      <header className="lang-card-head"><h3><BookOpen size={16} />学习进度</h3></header>
      {progress
        ? <ul className="lang-progress">
            <li>已学习 <b>{progress.total}</b></li>
            <li>学习中 <b>{progress.learning}</b></li>
            <li>已掌握 <b>{progress.mastered}</b></li>
            <li>复习次数 <b>{progress.reviews}</b></li>
            <li>收藏 <b>{progress.favorites}</b></li>
          </ul>
        : <p className="lang-empty">暂无学习记录。</p>}
    </section>

    <section className="lang-card">
      <header className="lang-card-head"><h3><Heart size={16} />收藏 <small>{favorites.length}</small></h3></header>
      {favorites.length === 0
        ? <p className="lang-empty">还没有收藏；在词条详情点击 ♥ 添加。</p>
        : <ul className="lang-hit-list">
            {favorites.filter((item) => item.language === language).map((item) =>
              <li key={item.id}>
                <button className="lang-hit" onClick={() => onOpen(item.id, false)}>
                  <span className="lang-hit-text">{item.text}</span>
                  {item.reading ? <i>{item.reading}</i> : null}
                  <small>{item.language}</small>
                </button>
              </li>)}
          </ul>}
      {favorites.length === 0 ? null : favorites.filter((item) => item.language === language).length === 0
        ? <p className="lang-muted">（当前语言 {language} 没有收藏；全部收藏 {favorites.length} 条。）</p>
        : null}
    </section>

    <section className="lang-card">
      <header className="lang-card-head"><h3><Star size={16} />数据来源与许可 <small>Settings → Language Data</small></h3></header>
      <p className="lang-muted">所有词条均可追溯来源（#5/#90）；Attribution 展示如下，许可能力位可展开。</p>
      <ul className="lang-sources">
        {sources.map((info) =>
          <li key={info.source.id}>
            <div className="lang-source-row">
              <b>{info.source.name}</b>
              <span>{info.source.license.kind}{info.source.license.attribution_required ? " · 需署名" : ""}</span>
              <small>{info.source.dataset_version} · {info.item_count} 条</small>
            </div>
            <p className="lang-muted">{info.source.attribution}</p>
            {info.manifest ? <small className="lang-license">清单：{info.manifest.name}（{info.manifest.imported_at && new Date(info.manifest.imported_at * 1000).toLocaleDateString()}）</small> : null}
          </li>)}
      </ul>
    </section>

    <section className="lang-card">
      <header className="lang-card-head"><h3>学习状态说明</h3></header>
      <p className="lang-muted">{(["new", "learning", "review", "mastered"] as const).map(stateLabel).join(" → ")}；
        学习状态与词典数据分离存储，#59。</p>
    </section>
  </div>;
}