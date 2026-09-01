import { invoke } from "@tauri-apps/api/core";
import {
  BookOpen, CalendarCheck, ListMagnifyingGlass, Microphone, Play, Star, Translate,
} from "@phosphor-icons/react";
import { useCallback, useEffect, useRef, useState } from "react";
import type {
  LanguageCode, LanguageInfo, LanguageItem, LearningStateKind, ReviewRating, SpeakingScore,
  StarterReport, TodayView, WordDetail,
} from "../../types";
import { errorMessage, isTauriRuntime } from "../../utils";
import { ExplorePanel, type OpenDetail } from "./ExplorePanel";
import { LibraryPanel } from "./LibraryPanel";
import { ListenPanel } from "./ListenPanel";
import { ReviewPanel } from "./ReviewPanel";
import { SpeakPanel } from "./SpeakPanel";
import { TodayPanel } from "./TodayPanel";
import { WordDetailView } from "./WordDetail";

export type LanguageTab = "today" | "explore" | "review" | "listen" | "speak" | "library";

export const LANGUAGE_CODES: LanguageCode[] = ["eng", "jpn", "cmn", "yue"];

interface LanguagePageProps {
  active: boolean;
  setNotice: (message: string) => void;
  intent?: { id: string; nonce: number } | null;
}

/**
 * Language Learning Hub（#62/#82/#83）：
 * 页面围绕学习目标组织（Today / Explore / Review / Listen / Speak / Library），
 * 数据全部来自本地 language.db；未安装数据包时引导安装 Starter Pack（离线可用）。
 */
export function LanguagePage({ active, setNotice, intent }: LanguagePageProps) {
  const [tab, setTab] = useState<LanguageTab>("today");
  const [language, setLanguage] = useState<LanguageCode>("jpn");
  const [languages, setLanguages] = useState<LanguageInfo[]>([]);
  const [hasData, setHasData] = useState(false);
  const [loading, setLoading] = useState(true);
  const [installing, setInstalling] = useState(false);
  const [selected, setSelected] = useState<WordDetail | null>(null);
  const [openIntent, setOpenIntent] = useState<{ id: string; nonce: number } | null>(null);
  const [refreshNonce, setRefreshNonce] = useState(0);

  const reload = useCallback(async () => {
    if (!isTauriRuntime()) return;
    try {
      const [info, today] = await Promise.all([
        invoke<LanguageInfo[]>("language_languages"),
        invoke<TodayView>("language_today", { language }),
      ]);
      setLanguages(info);
      setHasData(info.some((item) => item.total > 0));
      setLanguage((previous) => (today.language as LanguageCode) || previous);
    } catch (error) {
      setNotice(errorMessage(error));
    } finally {
      setLoading(false);
    }
  }, [language, setNotice]);

  // 首次进入 / 安装完成 / 语言切换时刷新
  const loadedOnce = useRef(false);
  useEffect(() => {
    if (!active) return;
    void reload();
    loadedOnce.current = true;
  }, [active, reload, refreshNonce]);

  const installStarter = useCallback(async () => {
    if (!isTauriRuntime() || installing) return;
    setInstalling(true);
    try {
      const report = await invoke<StarterReport>("language_install_starter", { only: null });
      setNotice(`Starter Pack 安装完成：+${report.total_inserted} 条（更新 ${report.total_updated}）`);
      setRefreshNonce((nonce) => nonce + 1);
      setTab("today");
    } catch (error) {
      setNotice(errorMessage(error));
    } finally {
      setInstalling(false);
    }
  }, [installing, setNotice]);

  const openDetail = useCallback<OpenDetail>((id: string | null, reset: boolean) => {
    if (reset) {
      setOpenIntent(null);
      setSelected(null);
      return;
    }
    if (id) {
      setOpenIntent({ id, nonce: Date.now() });
    }
  }, []);

  useEffect(() => {
    if (active && intent?.id && hasData) openDetail(intent.id, false);
  }, [active, hasData, intent, openDetail]);

  const onDetailUpdated = useCallback(() => {
    setRefreshNonce((nonce) => nonce + 1);
  }, []);

  if (loading) {
    return <div className="page-scroll lang-page"><p className="lang-empty">加载语言数据…</p></div>;
  }

  if (!hasData) {
    return <div className="page-scroll lang-page">
      <header className="lang-welcome">
        <Translate size={40} weight="duotone" />
        <h1>Language Learning Hub</h1>
        <p>已内置真实数据子集（Open English WordNet · CMUdict · JMdict · KANJIDIC2 · CC-CEDICT · words.hk · CC-Canto · Tatoeba），来源与许可完整可溯。</p>
      </header>
      <div className="lang-card lang-install-card">
        <h2>安装 Starter Pack</h2>
        <p>导入约 3 千条带 attribution 的真实词条与例句，完全离线可用；完整数据包可稍后通过
          <code> language-data import … </code>（见 docs/language/DATA_SOURCES.md）导入。</p>
        <button className="lang-primary" onClick={() => void installStarter()} disabled={installing}>
          {installing ? "安装中…" : "安装数据包"}
        </button>
      </div>
    </div>;
  }

  return <div className="page-scroll lang-page">
    <header className="lang-header">
      <div className="lang-title">
        <Translate size={22} />
        <h1>Language</h1>
        <select
          className="lang-select"
          value={language}
          onChange={(event) => setLanguage(event.target.value as LanguageCode)}
          aria-label="选择语言"
        >
          {LANGUAGE_CODES.map((code) => {
            const info = languages.find((item) => item.code === code);
            return <option key={code} value={code}>
              {info ? `${info.native_name} (${info.name})` : code}
            </option>;
          })}
        </select>
      </div>
      <nav className="lang-tabs" aria-label="语言学习功能">
        <button className={tab === "today" ? "active" : ""} onClick={() => setTab("today")}><CalendarCheck size={16} />Today</button>
        <button className={tab === "explore" ? "active" : ""} onClick={() => setTab("explore")}><ListMagnifyingGlass size={16} />Explore</button>
        <button className={tab === "review" ? "active" : ""} onClick={() => setTab("review")}><BookOpen size={16} />Review</button>
        <button className={tab === "listen" ? "active" : ""} onClick={() => setTab("listen")}><Play size={16} />Listen</button>
        <button className={tab === "speak" ? "active" : ""} onClick={() => setTab("speak")}><Microphone size={16} />Speak</button>
        <button className={tab === "library" ? "active" : ""} onClick={() => setTab("library")}><Star size={16} />Library</button>
      </nav>
    </header>

    {tab === "today" && <TodayPanel language={language} onStart={() => setTab("review")} onRefresh={() => setRefreshNonce((nonce) => nonce + 1)} setNotice={setNotice} />}
    {tab === "explore" && <ExplorePanel language={language} onOpen={openDetail} setNotice={setNotice} />}
    {tab === "review" && <ReviewPanel language={language} setNotice={setNotice} onOpen={openDetail} onUpdated={onDetailUpdated} />}
    {tab === "listen" && <ListenPanel language={language} setNotice={setNotice} />}
    {tab === "speak" && <SpeakPanel language={language} setNotice={setNotice} />}
    {tab === "library" && <LibraryPanel language={language} onOpen={openDetail} onUpdated={onDetailUpdated} setNotice={setNotice} />}

    {openIntent ? <WordDetailView
      itemId={openIntent.id}
      onClose={() => openDetail(null, true)}
      onUpdated={onDetailUpdated}
      setNotice={setNotice}
    /> : selected ? <WordDetailView
      itemId={selected.item.id}
      onClose={() => setSelected(null)}
      onUpdated={onDetailUpdated}
      setNotice={setNotice}
    /> : null}
  </div>;
}

export type { LearningStateKind, ReviewRating, SpeakingScore, LanguageItem };
