import { invoke } from "@tauri-apps/api/core";
import { ArrowLeft, Heart, Play, Star, X } from "@phosphor-icons/react";
import { useCallback, useEffect, useState } from "react";
import type { LearningStateKind, WordDetail } from "../../types";
import { errorMessage, isTauriRuntime } from "../../utils";
import { speak } from "./tts";

interface WordDetailProps {
  itemId: string;
  onClose: () => void;
  onUpdated: () => void;
  setNotice: (message: string) => void;
}

/** Word Detail（#63）：Word/Reading/Pronunciation/Meaning/POS/Examples/Related/Audio/Favorite/State/Source。 */
export function WordDetailView({ itemId, onClose, onUpdated, setNotice }: WordDetailProps) {
  const [detail, setDetail] = useState<WordDetail | null>(null);

  const reload = useCallback(async () => {
    if (!isTauriRuntime()) return;
    try {
      const result = await invoke<WordDetail | null>("language_item", { id: itemId });
      if (result) setDetail(result);
    } catch (error) {
      setNotice(errorMessage(error));
    }
  }, [itemId, setNotice]);

  useEffect(() => { void reload(); }, [reload]);

  if (!detail) return <p className="lang-empty">加载词条…</p>;
  const { item, meanings, pronunciations, relations, examples, sentences, state, favorite, source, kanji } = detail;

  const toggleFavorite = async () => {
    try {
      await invoke<boolean>("language_toggle_favorite", { itemId: item.id });
      onUpdated();
      void reload();
    } catch (error) { setNotice(errorMessage(error)); }
  };

  const setState = async (next: LearningStateKind) => {
    try {
      await invoke<void>("language_set_state", { request: { itemId: item.id, state: next } });
      onUpdated();
      void reload();
    } catch (error) { setNotice(errorMessage(error)); }
  };

  const readText = () => {
    const text = item.reading ?? item.text;
    if (!speak(text, item.language)) setNotice("当前设备无可用的 TTS 语音（不影响其他功能）。");
  };

  return <div className="lang-detail-overlay" role="dialog" aria-label="词条详情">
    <div className="lang-detail">
      <header className="lang-detail-head">
        <button onClick={onClose} title="返回"><ArrowLeft size={16} />返回</button>
        <span className="lang-state-chip">{state ? stateLabel(state.state) : "NEW"}</span>
        <button onClick={() => void toggleFavorite()} title={favorite ? "取消收藏" : "收藏"} className={favorite ? "lang-fav on" : "lang-fav"}>
          <Heart size={16} weight={favorite ? "fill" : "regular"} />
        </button>
      </header>

      <section className="lang-word-head">
        <h2>{item.text}</h2>
        {item.reading ? <p className="lang-reading">{item.reading}</p> : null}
        {item.romanization ? <p className="lang-roman">{item.romanization}</p> : null}
        <div className="lang-row-actions">
          <button onClick={readText} title="朗读"><Play size={15} />发音</button>
        </div>
      </section>

      {kanji ? <section className="lang-card">
        <h3>汉字信息 <small>KANJIDIC2</small></h3>
        <ul className="lang-kanji">
          {kanji.stroke_count == null ? null : <li>笔画 {kanji.stroke_count}</li>}
          {kanji.grade == null ? null : <li>学年 {kanji.grade}</li>}
          {kanji.radical == null ? null : <li>部⾸ {kanji.radical}</li>}
          {kanji.jlpt ? <li>JLPT（KANJIDIC2 标记）{kanji.jlpt}</li> : null}
        </ul>
      </section> : null}

      {pronunciations.length > 0 ? <section className="lang-card">
        <h3>发音</h3>
        <ul className="lang-prons">
          {pronunciations.map((pron) => <li key={pron.id}>
            <b>{pron.phonemes}</b>
            <small>{pron.scheme}{pron.variant ? ` · ${pron.variant}` : ""}{pron.tone == null ? "" : ` · tone ${pron.tone}`}</small>
          </li>)}
        </ul>
      </section> : null}

      <section className="lang-card">
        <h3>释义 {meanings.length > 0 ? <small>{meanings.length} 义项</small> : null}</h3>
        {meanings.length === 0
          ? <p className="lang-empty">暂无释义{source ? `（来源 ${source.name} 仅提供发音/转写）` : ""}。</p>
          : <ol className="lang-meanings">
              {meanings.map((meaning) => <li key={meaning.id}>
                {meaning.pos ? <span className="lang-pos">{meaning.pos}</span> : null}
                {meaning.gloss ? <p>{meaning.gloss}</p> : null}
                {meaning.raw && meaning.raw !== meaning.gloss ? <small>{meaning.raw}</small> : null}
              </li>)}
            </ol>}
      </section>

      {examples.length > 0 ? <section className="lang-card">
        <h3>例句</h3>
        <ul className="lang-examples">
          {examples.map((example) => <li key={example.text}>
            <button className="lang-sentence" onClick={() => void speak(example.text, item.language)}>
              <Play size={13} />
              <span>{example.text}</span>
            </button>
          </li>)}
        </ul>
      </section> : null}

      {sentences.length > 0 ? <section className="lang-card">
        <h3>相关句子 <small>Tatoeba</small></h3>
        <ul className="lang-examples">
          {sentences.map((sentence) => <li key={sentence.sentence_id}>
            <button className="lang-sentence" onClick={() => void speak(sentence.text, sentence.language)}>
              <Play size={13} />
              <span>{sentence.text}</span>
            </button>
            <small className="lang-license">{sentence.license} · #{sentence.sentence_id}{sentence.author ? ` · ${sentence.author}` : ""}</small>
          </li>)}
        </ul>
      </section> : null}

      {relations.length > 0 ? <section className="lang-card">
        <h3>相关词</h3>
        <ul className="lang-related">
          {relations.map((view) => <li key={view.relation.id}>
            <button onClick={() => navigateTo(view.item.id)}>
              <span className="lang-pos">{view.label}</span>
              {view.item.text}
              {view.item.reading ? <i>{view.item.reading}</i> : null}
            </button>
          </li>)}
        </ul>
      </section> : null}

      <section className="lang-card">
        <h3>学习状态</h3>
        <div className="lang-state-actions">
          {(["new", "learning", "review", "mastered"] as LearningStateKind[]).map((kind) =>
            <button key={kind} className={state?.state === kind ? "active" : ""} onClick={() => void setState(kind)}>
              {stateLabel(kind)}
            </button>)}
        </div>
      </section>

      {source ? <footer className="lang-source">
        <Star size={13} />
        <div>
          <b>{source.name}</b>
          <small>{source.dataset_version} · {source.license.kind}
            {source.license.attribution_required ? " · 需署名" : ""}
            {source.license.commercial_use_allowed ? " · 可商用" : " · 非商用"}</small>
        </div>
        <button title="关闭" onClick={onClose}><X size={14} /></button>
      </footer> : null}
    </div>
  </div>;

  function navigateTo(id: string) {
    // 简单实现：同页加载新词条（通过 key 变化触发 reload）
    setDetail(null);
    // 由 parent 持有 itemId；这里直接重新拉取
    void invoke<WordDetail | null>("language_item", { id }).then((result) => {
      if (result) setDetail(result);
    }).catch((error) => setNotice(errorMessage(error)));
  }
}

export function stateLabel(state: LearningStateKind): string {
  switch (state) {
    case "learning": return "学习中";
    case "review": return "复习中";
    case "mastered": return "已掌握";
    default: return "新词";
  }
}