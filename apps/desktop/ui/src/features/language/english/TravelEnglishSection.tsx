import { useCallback, useEffect, useMemo, useState } from "react";
import {
  ArrowLeft,
  BookOpen,
  CheckCircle,
  MagnifyingGlass,
  SpeakerHigh,
} from "@phosphor-icons/react";
import type { LearningProgress } from "../../../types";
import { errorMessage } from "../../../utils";
import { learningClient } from "../../learning/learningClient";
import {
  ARTICLES,
  GRAMMAR_LESSONS,
  SCENES,
  WORDS,
  WORD_TAGS,
} from "./travelContent";
import { RevealLine } from "./RevealLine";

const TRAVEL_TYPES = new Set(["travel_scene", "travel_word", "travel_grammar", "travel_article"]);

type TravelTab = "scenes" | "words" | "grammar" | "reading" | "progress";
type ActivityAction = "study" | "complete" | "correct" | "incorrect";

const TABS: ReadonlyArray<{ id: TravelTab; label: string }> = [
  { id: "scenes", label: "旅行场景" },
  { id: "words", label: "词汇" },
  { id: "grammar", label: "语法" },
  { id: "reading", label: "阅读" },
  { id: "progress", label: "学习进度" },
];

function entityKey(type: string, id: string) {
  return `${type}:${id}`;
}

function slug(value: string) {
  return value.trim().toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "");
}

function wordEntityId(word: (typeof WORDS)[number]) {
  return `travel:${word.tag}:${slug(word.en)}:${slug(word.ex)}`;
}

function statusLabel(status: LearningProgress["status"]) {
  switch (status) {
    case "learning": return "学习中";
    case "familiar": return "已熟悉";
    case "mastered": return "已掌握";
    default: return "未开始";
  }
}

function speak(text: string) {
  if (!("speechSynthesis" in window) || typeof SpeechSynthesisUtterance === "undefined") return false;
  window.speechSynthesis.cancel();
  const utterance = new SpeechSynthesisUtterance(text);
  utterance.lang = "en-US";
  window.speechSynthesis.speak(utterance);
  return true;
}

export interface TravelEnglishSectionProps {
  onBack: () => void;
}

export function TravelEnglishSection({ onBack }: TravelEnglishSectionProps) {
  const [tab, setTab] = useState<TravelTab>("scenes");
  const [progress, setProgress] = useState<LearningProgress[]>([]);
  const [loadingProgress, setLoadingProgress] = useState(true);
  const [progressError, setProgressError] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [savingKey, setSavingKey] = useState<string | null>(null);
  const [wordQuery, setWordQuery] = useState("");
  const [wordTag, setWordTag] = useState<string>("all");
  const [answers, setAnswers] = useState<Record<string, number>>({});
  // 主动回忆：默认隐藏译文，可整组显示；列表分批渲染，避免一次铺满整屏。
  const [revealAll, setRevealAll] = useState(false);
  const [expandedSay, setExpandedSay] = useState<Record<string, boolean>>({});
  const [sceneLimit, setSceneLimit] = useState(4);
  const [wordLimit, setWordLimit] = useState(24);
  const [grammarLimit, setGrammarLimit] = useState(3);
  const [articleLimit, setArticleLimit] = useState(4);

  const loadProgress = useCallback(async () => {
    setLoadingProgress(true);
    setProgressError(null);
    try {
      const items = await learningClient.listProgress("language", undefined, 500);
      setProgress(items.filter((item) => TRAVEL_TYPES.has(item.entity_type) && item.entity_id.startsWith("travel:")));
    } catch (cause) {
      setProgressError(errorMessage(cause));
    } finally {
      setLoadingProgress(false);
    }
  }, []);

  useEffect(() => {
    void loadProgress();
    return () => {
      if ("speechSynthesis" in window) window.speechSynthesis.cancel();
    };
  }, [loadProgress]);

  const progressByEntity = useMemo(
    () => new Map(progress.map((item) => [entityKey(item.entity_type, item.entity_id), item])),
    [progress],
  );

  const recordActivity = useCallback(async (
    entityType: string,
    entityId: string,
    title: string,
    action: ActivityAction,
    metadata?: Record<string, unknown>,
  ) => {
    const key = entityKey(entityType, entityId);
    setSavingKey(key);
    setActionError(null);
    try {
      const updated = await learningClient.recordEvent({
        module: "language",
        entity_type: entityType,
        entity_id: entityId,
        entity_title: title,
        action,
        source: "travel-english-pwa",
        metadata,
      });
      setProgress((current) => [updated, ...current.filter((item) => item.entity_key !== updated.entity_key)]);
      return true;
    } catch (cause) {
      setActionError(`进度没有保存：${errorMessage(cause)}`);
      return false;
    } finally {
      setSavingKey(null);
    }
  }, []);

  const filteredWords = useMemo(() => {
    const query = wordQuery.trim().toLowerCase();
    return WORDS.filter((word) => (wordTag === "all" || word.tag === wordTag) && (
      !query || [word.en, word.zh, word.ex, word.exZh].some((value) => value.toLowerCase().includes(query))
    ));
  }, [wordQuery, wordTag]);

  const studiedCount = progress.filter((item) => item.study_count > 0 || item.review_count > 0 || item.correct_count > 0 || item.incorrect_count > 0).length;
  const averageMastery = progress.length
    ? Math.round(progress.reduce((sum, item) => sum + item.mastery_score, 0) / progress.length)
    : 0;

  const answerQuestion = async (lessonId: string, lessonTitle: string, questionIndex: number, selected: number, correct: number) => {
    const questionKey = `${lessonId}:${questionIndex}`;
    if (answers[questionKey] !== undefined) return;
    setAnswers((current) => ({ ...current, [questionKey]: selected }));
    await recordActivity(
      "travel_grammar",
      `travel:${lessonId}`,
      lessonTitle,
      selected === correct ? "correct" : "incorrect",
      { question_index: questionIndex, selected_answer: selected },
    );
  };

  const speakText = (text: string) => {
    if (!speak(text)) setActionError("当前环境不支持英语朗读。");
  };

  return (
    <section className="en-travel" aria-labelledby="en-travel-title">
      <header className="en-travel-header">
        <button type="button" className="en-ghost-btn en-travel-back" onClick={onBack}>
          <ArrowLeft size={16} aria-hidden="true" /> 返回英语首页
        </button>
        <div>
          <p className="en-travel-eyebrow">TRAVEL ENGLISH</p>
          <h1 id="en-travel-title">旅行英语，开口前先练一遍</h1>
          <p className="en-muted">场景对话、随身词汇、语法和短篇阅读。学习记录会保存到统一进度。</p>
        </div>
      </header>

      <nav className="en-travel-tabs" role="tablist" aria-label="旅行英语内容">
        {TABS.map((item) => (
          <button
            key={item.id}
            type="button"
            role="tab"
            aria-selected={tab === item.id}
            className={`en-travel-tab${tab === item.id ? " is-active" : ""}`}
            onClick={() => setTab(item.id)}
          >
            {item.label}
          </button>
        ))}
      </nav>

      {actionError ? <p className="en-travel-error" role="status">{actionError}</p> : null}
      {progressError ? (
        <div className="en-travel-error" role="alert">
          <span>进度暂时无法读取：{progressError}</span>
          <button type="button" className="en-link-btn" onClick={() => void loadProgress()}>重试</button>
        </div>
      ) : null}

      <div
        className="en-travel-panel"
        role="tabpanel"
        key={`${tab}-${revealAll ? "revealed" : "hidden"}`}
      >
        {tab === "scenes" || tab === "words" || tab === "reading" ? (
          <div className="en-travel-panel-bar">
            <p className="en-travel-panel-hint">
              先试着回忆中文意思，再点开核对——主动回忆比同时看中英文记得更牢。
            </p>
            <button
              type="button"
              className="en-travel-reveal-all"
              aria-pressed={revealAll}
              onClick={() => setRevealAll((value) => !value)}
            >
              {revealAll ? "隐藏全部译文" : "显示全部译文"}
            </button>
          </div>
        ) : null}
        {tab === "scenes" ? (
          <div className="en-travel-stack">
          <div className="en-travel-grid">
            {SCENES.slice(0, sceneLimit).map((scene) => {
              const id = `travel:${scene.id}`;
              const saved = progressByEntity.get(entityKey("travel_scene", id));
              return (
                <article className="en-card en-travel-card" key={scene.id}>
                  <header className="en-travel-card-head">
                    <div><p className="en-travel-eyebrow">{scene.en}</p><h2>{scene.name}</h2></div>
                    {saved ? <span className="en-chip">{statusLabel(saved.status)}</span> : null}
                  </header>
                  <p className="en-muted">{scene.desc}</p>
                  {scene.say.length ? (
                    <div className="en-travel-lines">
                      <h3>先开口说 <small>{scene.say.length} 句</small></h3>
                      {(expandedSay[scene.id] ? scene.say : scene.say.slice(0, 3)).map(([english, chinese], index) => (
                        <RevealLine
                          key={`${scene.id}-say-${index}`}
                          english={english}
                          chinese={chinese}
                          defaultRevealed={revealAll}
                          onSpeak={() => speakText(english)}
                        />
                      ))}
                      {scene.say.length > 3 ? (
                        <button
                          type="button"
                          className="en-link-btn"
                          onClick={() =>
                            setExpandedSay((current) => ({ ...current, [scene.id]: !current[scene.id] }))
                          }
                        >
                          {expandedSay[scene.id] ? "收起" : `展开全部 ${scene.say.length} 句`}
                        </button>
                      ) : null}
                    </div>
                  ) : null}
                  {scene.listen.length ? (
                    <details className="en-travel-details"><summary>听力练习（{scene.listen.length} 句）</summary>
                      {scene.listen.map(([english, chinese], index) => <RevealLine key={`${scene.id}-listen-${index}`} english={english} chinese={chinese} defaultRevealed={revealAll} onSpeak={() => speakText(english)} />)}
                    </details>
                  ) : null}
                  {scene.dialogue.length ? (
                    <details className="en-travel-details"><summary>展开情景对话</summary>
                      {scene.dialogue.map(([speaker, english, chinese], index) => <RevealLine className={`en-travel-dialogue is-${speaker}`} key={`${scene.id}-dialogue-${index}`} english={english} chinese={chinese} defaultRevealed={revealAll} onSpeak={() => speakText(english)} />)}
                    </details>
                  ) : null}
                  {scene.notes.length ? (
                    <details className="en-travel-details"><summary>表达提示</summary>
                      {scene.notes.map(([english, chinese], index) => <RevealLine key={`${scene.id}-note-${index}`} english={english} chinese={chinese} defaultRevealed={revealAll} onSpeak={() => speakText(english)} />)}
                    </details>
                  ) : null}
                  <button
                    type="button"
                    className="en-primary-btn"
                    disabled={savingKey === entityKey("travel_scene", id)}
                    onClick={() => void recordActivity("travel_scene", id, scene.name, "study", { scene_id: scene.id })}
                  >
                    {savingKey === entityKey("travel_scene", id) ? "保存中…" : "记录本次练习"}
                  </button>
                </article>
              );
            })}
          </div>
          {SCENES.length > sceneLimit ? (
            <button
              type="button"
              className="en-ghost-btn en-travel-more"
              onClick={() => setSceneLimit((value) => value + 4)}
            >
              显示更多场景（还剩 {SCENES.length - sceneLimit} 个）
            </button>
          ) : null}
          </div>
        ) : null}

        {tab === "words" ? (
          <div className="en-travel-stack">
            <div className="en-travel-toolbar">
              <label className="en-travel-search"><MagnifyingGlass size={17} aria-hidden="true" /><input className="en-input" value={wordQuery} onChange={(event) => setWordQuery(event.target.value)} placeholder="搜索英文、中文或例句" aria-label="搜索旅行英语词汇" /></label>
              <span className="en-muted">{filteredWords.length} / {WORDS.length} 个词</span>
            </div>
            <div className="en-travel-filters" aria-label="词汇分类">
              {WORD_TAGS.map((tag) => <button key={tag.id} type="button" className={`en-chip en-travel-filter${wordTag === tag.id ? " is-active" : ""}`} aria-pressed={wordTag === tag.id} onClick={() => setWordTag(tag.id)}>{tag.label}</button>)}
            </div>
            <div className="en-travel-grid">
              {filteredWords.slice(0, wordLimit).map((word) => {
                const id = wordEntityId(word);
                const saved = progressByEntity.get(entityKey("travel_word", id));
                return (
                  <article className="en-card en-travel-word" key={id}>
                    <div className="en-travel-card-head"><div><h2>{word.en}</h2><span className="en-travel-pos">{word.pos}</span> <span className="en-travel-pos">{word.zh}</span></div>{saved ? <span className="en-chip">{statusLabel(saved.status)}</span> : null}</div>
                    <RevealLine className="en-travel-example" english={word.ex} chinese={word.exZh} defaultRevealed={revealAll} onSpeak={() => speakText(word.ex)} />
                    <div className="en-travel-actions">
                      <button type="button" className="en-icon-btn" aria-label={`朗读：${word.en}`} onClick={() => speakText(word.en)}><SpeakerHigh size={16} /> 朗读</button>
                      <button type="button" className="en-ghost-btn" disabled={savingKey === entityKey("travel_word", id)} onClick={() => void recordActivity("travel_word", id, word.en, "study", { category: word.tag })}>{savingKey === entityKey("travel_word", id) ? "保存中…" : "记录练习"}</button>
                    </div>
                  </article>
                );
              })}
            </div>
            {filteredWords.length === 0 ? <p className="en-muted">没有找到匹配的词，试试缩短关键词或切换分类。</p> : null}
            {filteredWords.length > wordLimit ? (
              <button
                type="button"
                className="en-ghost-btn en-travel-more"
                onClick={() => setWordLimit((value) => value + 24)}
              >
                再显示 24 个（还剩 {filteredWords.length - wordLimit} 个）
              </button>
            ) : null}
          </div>
        ) : null}

        {tab === "grammar" ? (
          <div className="en-travel-stack">
            {GRAMMAR_LESSONS.slice(0, grammarLimit).map((lesson) => {
              const id = `travel:${lesson.id}`;
              const saved = progressByEntity.get(entityKey("travel_grammar", id));
              return (
                <article className="en-card en-travel-grammar" key={lesson.id}>
                  <header className="en-travel-card-head"><div><p className="en-travel-eyebrow">{lesson.sub}</p><h2>{lesson.title}</h2></div>{saved ? <span className="en-chip">{statusLabel(saved.status)}</span> : null}</header>
                  <p className="en-travel-rule">{lesson.rule}</p>
                  <div className="en-travel-pairs">
                    {lesson.pairs.map(([incorrect, correction, explanation], index) => <div className="en-travel-pair" key={`${lesson.id}-pair-${index}`}><del>{incorrect}</del><strong>{correction}</strong><span>{explanation}</span></div>)}
                  </div>
                  <h3>试着选出自然的表达</h3>
                  {lesson.quiz.map((question, questionIndex) => {
                    const questionKey = `${lesson.id}:${questionIndex}`;
                    const selected = answers[questionKey];
                    return (
                      <fieldset className="en-travel-question" key={questionKey}>
                        <legend>{question.q}</legend>
                        {question.opts.map((option, optionIndex) => {
                          const answered = selected !== undefined;
                          const correct = answered && optionIndex === question.a;
                          const chosenWrong = answered && selected === optionIndex && selected !== question.a;
                          return (
                            <button key={optionIndex} type="button" className={`en-travel-option${correct ? " is-correct" : ""}${chosenWrong ? " is-wrong" : ""}`} aria-pressed={selected === optionIndex} disabled={answered} onClick={() => void answerQuestion(lesson.id, lesson.title, questionIndex, optionIndex, question.a)}>
                              {correct ? <CheckCircle size={16} aria-hidden="true" /> : null}{option}
                            </button>
                          );
                        })}
                        {selected !== undefined ? <p className={selected === question.a ? "en-travel-feedback is-correct" : "en-travel-feedback is-wrong"} role="status">{selected === question.a ? "回答正确" : `正确答案：${question.opts[question.a]}`}</p> : null}
                      </fieldset>
                    );
                  })}
                </article>
              );
            })}
            {GRAMMAR_LESSONS.length > grammarLimit ? (
              <button
                type="button"
                className="en-ghost-btn en-travel-more"
                onClick={() => setGrammarLimit((value) => value + 2)}
              >
                显示更多语法课（还剩 {GRAMMAR_LESSONS.length - grammarLimit} 节）
              </button>
            ) : null}
          </div>
        ) : null}

        {tab === "reading" ? (
          <div className="en-travel-stack">
          <div className="en-travel-grid">
            {ARTICLES.slice(0, articleLimit).map((article) => {
              const id = `travel:${article.id}`;
              const saved = progressByEntity.get(entityKey("travel_article", id));
              return (
                <article className="en-card en-travel-article" key={article.id}>
                  <header className="en-travel-card-head"><div><p className="en-travel-eyebrow">{article.level} · {article.tag}</p><h2>{article.title}</h2><p className="en-muted">{article.titleZh}</p></div>{saved ? <span className="en-chip">{statusLabel(saved.status)}</span> : null}</header>
                  <div className="en-travel-paragraphs">
                    {article.paras.map(([english, chinese], index) => <details className="en-travel-paragraph" key={`${article.id}-paragraph-${index}`}><summary><BookOpen size={15} aria-hidden="true" /> {english}</summary><p>{chinese}</p><button type="button" className="en-link-btn" onClick={() => speakText(english)}>朗读这段</button></details>)}
                  </div>
                  <button type="button" className="en-primary-btn" disabled={savingKey === entityKey("travel_article", id)} onClick={() => void recordActivity("travel_article", id, article.title, "complete", { article_id: article.id })}>{savingKey === entityKey("travel_article", id) ? "保存中…" : "记录读完"}</button>
                </article>
              );
            })}
          </div>
          {ARTICLES.length > articleLimit ? (
            <button
              type="button"
              className="en-ghost-btn en-travel-more"
              onClick={() => setArticleLimit((value) => value + 4)}
            >
              显示更多短文（还剩 {ARTICLES.length - articleLimit} 篇）
            </button>
          ) : null}
          </div>
        ) : null}

        {tab === "progress" ? (
          <section className="en-travel-stack" aria-labelledby="en-travel-progress-title">
            <div className="en-travel-metrics">
              <article className="en-card en-travel-stat"><span>有学习记录</span><strong>{loadingProgress ? "…" : studiedCount}</strong><small>个旅行英语内容</small></article>
              <article className="en-card en-travel-stat"><span>平均掌握度</span><strong>{loadingProgress || progress.length === 0 ? "—" : `${averageMastery}%`}</strong><small>按服务端进度计算</small></article>
              <article className="en-card en-travel-stat"><span>已记录答题</span><strong>{loadingProgress ? "…" : progress.reduce((sum, item) => sum + item.correct_count + item.incorrect_count, 0)}</strong><small>正确与错误次数合计</small></article>
            </div>
            {loadingProgress ? <p className="en-muted">正在读取已保存的学习进度…</p> : null}
            {!loadingProgress && !progressError && progress.length === 0 ? <p className="en-muted">还没有旅行英语学习记录。完成一个场景、词汇练习或阅读后，记录会显示在这里。</p> : null}
            <div className="en-travel-stack">
              {[...progress].sort((a, b) => b.last_studied_at - a.last_studied_at).slice(0, 30).map((item) => <article className="en-card en-travel-progress-item" key={item.entity_key}><div><span className="en-chip">{statusLabel(item.status)}</span><h3>{item.entity_title}</h3><p className="en-muted">学习 {item.study_count} 次 · 正确 {item.correct_count} · 错误 {item.incorrect_count}</p></div><strong>{item.mastery_score}%</strong></article>)}
            </div>
            <button type="button" className="en-ghost-btn" disabled={loadingProgress} onClick={() => void loadProgress()}>刷新已保存的进度</button>
          </section>
        ) : null}
      </div>
    </section>
  );
}
