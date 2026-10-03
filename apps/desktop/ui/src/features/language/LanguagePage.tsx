/**
 * Language 模块入口。
 *
 * 负责三件事：
 * 1. 顶部：语言选择 + 分区 Tab（首页 / 探索 / 复习 / 错题 / 句库 / 课程 / 词库来源）。
 * 2. 分区渲染：每块自己管自己的 loading / error / 空态。
 * 3. 覆盖层：专注模式、课程播放器、词典详情、复习会话——它们盖住整个页面，
 *    不显示侧栏 / Tab 栏 / 语言选择器。
 *
 * 数据全部走 `languageClient`；`learningClient` 只用来列合集（合集是平台能力）。
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { DownloadSimple, Translate } from "@phosphor-icons/react";
import type {
  LanguageCode,
  LanguageInfo,
  LanguageLearningItem,
  Lesson,
  Mistake,
  ReviewQueueItem,
  SentenceRecord,
  SentenceStudy,
  SourceInfo,
  StarterReport,
} from "../../types";
import type { AppContextPayload } from "../ai/aiTypes";
import { errorMessage } from "../../utils";
import { languageClient } from "./languageClient";
import { Action, Chip, Panel, PanelBody, Skeleton } from "./LanguagePrimitives";
import { ExplorePanel } from "./ExplorePanel";
import { FocusMode } from "./FocusMode";
import { LanguageHome } from "./LanguageHome";
import { LessonPlayer } from "./LessonPlayer";
import { StudyCardSession } from "./StudyCardSession";
import { MistakesPanel, ReviewSession } from "./ReviewSession";
import type { StudyTarget } from "./ItemViews";
import { WordDetailView } from "./WordDetailView";
import {
  EMPTY_COPY,
  LANGUAGE_LABELS,
  formatStamp,
  useAsyncPanel,
} from "./languageUi";

export type LanguageTab =
  /** 默认视图：进 Language 先给学习卡片。 */
  | "study"
  | "home"
  | "explore"
  | "review"
  | "mistakes"
  | "sentences"
  | "lessons"
  | "sources";

export const LANGUAGE_CODES: LanguageCode[] = ["eng", "jpn", "cmn", "yue"];

interface LanguagePageProps {
  active: boolean;
  setNotice: (message: string) => void;
  intent?: { id: string; nonce: number } | null;
  /** 打开全局 AI 并把提示词送过去；缺省时 UI 隐藏 AI 入口（不是禁用成灰）。 */
  onAskAi?: (prompt: string) => void;
}

/**
 * 覆盖层。是一个**栈**：在专注模式里点开词典详情、再点返回时，
 * 必须回到原来的专注内容，而不是把它丢掉。
 */
type Overlay =
  | { kind: "focus"; target: StudyTarget; position: { index: number; total: number } | null }
  | { kind: "lesson"; lessonId: string }
  | { kind: "review" }
  | { kind: "detail"; itemId: string }
  | null;

export function LanguagePage({
  active,
  setNotice,
  intent,
  onContextChange,
  onAskAi,
}: LanguagePageProps & {
  onContextChange?: (ctx: AppContextPayload | null) => void;
}) {
  const [tab, setTab] = useState<LanguageTab>("study");
  const [language, setLanguage] = useState<LanguageCode>("jpn");
  // 栈顶即当前显示的覆盖层；`null` 栈表示没有覆盖层。
  const [overlays, setOverlays] = useState<Overlay[]>([]);
  const overlay = overlays.length > 0 ? overlays[overlays.length - 1] : null;
  const pushOverlay = useCallback((next: Overlay) => {
    setOverlays((previous) => [...previous, next]);
  }, []);
  const popOverlay = useCallback(() => {
    setOverlays((previous) => previous.slice(0, -1));
  }, []);
  const closeOverlay = useCallback(() => setOverlays([]), []);
  const [refreshToken, setRefreshToken] = useState(0);
  const [openIntent, setOpenIntent] = useState<{ id: string; nonce: number } | null>(null);
  const [lessonDraft, setLessonDraft] = useState<
    Array<{ id: string; text: string }>
  >([]);
  const [lessonTitle, setLessonTitle] = useState("");
  const [creatingLesson, setCreatingLesson] = useState(false);

  const bump = useCallback(() => setRefreshToken((n) => n + 1), []);

  // ---------------------------------------------------------------- 语言列表
  const languages = useAsyncPanel<LanguageInfo[]>(
    () => languageClient.languages(),
    [],
  );

  // ---------------------------------------------------------------- AI 上下文
  useEffect(() => {
    onContextChange?.({
      module: "language",
      page: tab,
      entity: null,
      // `view_state` 是 AI 契约里承载视图上下文的字段（`extra` 不在契约内）。
      view_state: { language },
    });
  }, [language, onContextChange, tab]);

  // ---------------------------------------------------------------- 外部意图
  useEffect(() => {
    if (!intent) return;
    setOpenIntent(intent);
  }, [intent]);

  useEffect(() => {
    if (!openIntent) return;
    setTab("explore");
    setOverlays([{ kind: "detail", itemId: openIntent.id }]);
  }, [openIntent]);

  // ---------------------------------------------------------------- 队列 / 错题
  const queue = useAsyncPanel<ReviewQueueItem[]>(
    () => languageClient.reviewQueue(50),
    [refreshToken],
  );
  const mistakes = useAsyncPanel<Mistake[]>(
    () => languageClient.mistakes(50),
    [refreshToken],
  );

  // ---------------------------------------------------------------- 覆盖层加载
  /** 首页点「去学这条」时用 `learningItem` 拿到条目再开专注模式。 */
  const [pendingItemId, setPendingItemId] = useState<string | null>(null);
  const pendingItem = useAsyncPanel<LanguageLearningItem | null>(
    () =>
      pendingItemId
        ? languageClient.learningItem(pendingItemId)
        : Promise.resolve(null),
    [pendingItemId],
    pendingItemId !== null,
  );

  useEffect(() => {
    if (pendingItemId === null) return;
    if (pendingItem.loading || pendingItem.error) return;
    if (pendingItem.data) {
      pushOverlay({
        kind: "focus",
        target: { kind: "item", item: pendingItem.data },
        position: null,
      });
    }
    setPendingItemId(null);
  }, [
    pendingItemId,
    pendingItem.data,
    pendingItem.error,
    pendingItem.loading,
    pushOverlay,
  ]);

  /**
   * 打开句子学习：直接用**句子 id** 调 `sentenceStudy`。
   *
   * 此前这里先拿句子原文去 `search()` 反查 id —— 文章里点开的一句未必能被搜索
   * 命中（分词 / 罗马字差异都会让它落空），于是「逐句学习」在真实数据上经常
   * 静默退化成空视图。句库列表里本来就有 `sentence_id`，没有理由绕一圈。
   */
  const [pendingSentenceId, setPendingSentenceId] = useState<string | null>(null);
  /**
   * 文章里点开的一句只有**原文**（文章正文没有句子 id）。
   * 这里显式做一次「原文 → 句子 id」解析：命中就开句子学习，搜不到就给出可见说明，
   * 而不是像之前那样把解析塞进取数据的 useAsyncPanel 里——解析失败会静默变成空视图。
   */
  const [pendingSentenceText, setPendingSentenceText] = useState<{
    text: string;
    language: LanguageCode;
  } | null>(null);
  const [sentenceResolveError, setSentenceResolveError] = useState<string | null>(null);

  // 自增 token：连点两句话时，慢的那次响应不能覆盖快的，否则会打开另一句。
  const sentenceToken = useRef(0);
  const openSentenceText = useCallback(
    async (text: string, language: LanguageCode) => {
      const token = ++sentenceToken.current;
      setSentenceResolveError(null);
      try {
        const hits = await languageClient.search(language, text, 8);
        if (token !== sentenceToken.current) return;
        const match = hits.find((hit) => hit.item.item_type === "SENTENCE");
        if (match) {
          setPendingSentenceId(match.item.id);
        } else {
          setSentenceResolveError("词典里没有找到这句话的记录，无法拆解。原文仍可继续阅读。");
        }
      } catch (error) {
        if (token !== sentenceToken.current) return;
        setSentenceResolveError(errorMessage(error));
      }
    },
    [],
  );
  const sentenceStudy = useAsyncPanel<SentenceStudy | null>(
    () =>
      pendingSentenceId
        ? languageClient.sentenceStudy(pendingSentenceId)
        : Promise.resolve(null),
    [pendingSentenceId],
    pendingSentenceId !== null,
  );

  useEffect(() => {
    if (!pendingSentenceId) return;
    if (sentenceStudy.loading) return;
    if (sentenceStudy.data) {
      pushOverlay({
        kind: "focus",
        target: { kind: "sentence", sentence: sentenceStudy.data },
        position: null,
      });
      setPendingSentenceId(null);
    }
  }, [
    pendingSentenceId,
    sentenceStudy.loading,
    sentenceStudy.data,
    pushOverlay,
  ]);

  // ---------------------------------------------------------------- 课程
  const lessons = useAsyncPanel<Lesson[]>(
    () => languageClient.lessons(language, 50),
    [language, refreshToken],
  );

  const createLesson = async () => {
    if (lessonDraft.length === 0) return;
    setCreatingLesson(true);
    try {
      const created = await languageClient.createLesson({
        language,
        title: lessonTitle.trim() || `${LANGUAGE_LABELS[language]} · ${new Date().toLocaleDateString()}`,
        itemIds: lessonDraft.map((entry) => entry.id),
      });
      setLessonDraft([]);
      setLessonTitle("");
      setTab("lessons");
      bump();
      setNotice(`已创建课程《${created.title}》，共 ${created.steps.length} 步。`);
    } catch (error) {
      setNotice(errorMessage(error));
    } finally {
      setCreatingLesson(false);
    }
  };

  const removeLesson = async (lesson: Lesson) => {
    try {
      await languageClient.deleteLesson(lesson.id);
      bump();
      setNotice(`已删除课程《${lesson.title}》。`);
    } catch (error) {
      setNotice(errorMessage(error));
    }
  };

  // ---------------------------------------------------------------- Starter Pack
  const [installing, setInstalling] = useState(false);
  const installStarter = async () => {
    setInstalling(true);
    try {
      const report: StarterReport = await languageClient.installStarter();
      languages.reload();
      sentences.reload();
      sources.reload();
      bump();
      setNotice(
        `Starter Pack 安装完成：新增 ${report.total_inserted} 条，更新 ${report.total_updated} 条。`,
      );
    } catch (error) {
      setNotice(errorMessage(error));
    } finally {
      setInstalling(false);
    }
  };

  const sentences = useAsyncPanel<SentenceRecord[]>(
    () => languageClient.sentences(language, 30),
    [language, refreshToken],
  );
  const sources = useAsyncPanel<SourceInfo[]>(
    () => languageClient.sources(),
    [refreshToken],
  );

  const hasData = (languages.data ?? []).some((row) => row.total > 0);

  // ---------------------------------------------------------------- 渲染
  const openSentenceById = useCallback(
    (sentenceId: string) => {
      setPendingSentenceId(sentenceId);
    },
    [],
  );

  const detailOverlay =
    overlay?.kind === "detail" ? (
      <WordDetailView
        itemId={overlay.itemId}
        onClose={() => closeOverlay()}
        onAskAi={undefined}
        // 收藏后的「已收藏」状态由复习队列推导，详情页只需触发一次刷新。
        onBookmarked={bump}
        onAddToLesson={(id, text) =>
          setLessonDraft((previous) =>
            previous.some((entry) => entry.id === id)
              ? previous
              : [...previous, { id, text }],
          )
        }
      />
    ) : null;

  const overlayNode = (() => {
    if (overlay?.kind === "focus") {
      // 收窄到 item 后再算「已收藏」，闭包里 TS 不会自动保留判别式收窄。
      const focusEntityId =
        overlay.target.kind === "item" ? overlay.target.item.id : null;
      return (
        <FocusMode
          target={overlay.target}
          position={overlay.position}
          // 「已收藏」必须来自**服务端真值**（复习队列里是否已有该条目的卡），
          // 此前是一个只写不读的本地 Set：刷新后永远显示未收藏，
          // 且在专注模式里点收藏成功后图标也不会变。
          bookmarked={
            focusEntityId !== null &&
            (queue.data?.some(
              (row) => row.card.entity_id === focusEntityId,
            ) ?? false)
          }
          onKnow={bump}
          onNeedReview={() => {
            bump();
            queue.reload();
          }}
          onBookmark={() => {
            bump();
            queue.reload();
          }}
          onClose={() => closeOverlay()}
          onOpenWordDetail={(entityId) =>
            pushOverlay({ kind: "detail", itemId: entityId })
          }
          onOpenSentence={(text) =>
            void openSentenceText(text, currentLanguage(overlay.target))
          }
          onNext={() => closeOverlay()}
        />
      );
    }
    if (overlay?.kind === "lesson") {
      return (
        <LessonPlayer
          lessonId={overlay.lessonId}
          onClose={() => {
            popOverlay();
            bump();
          }}
          onProgressSaved={bump}
        />
      );
    }
    if (overlay?.kind === "review") {
      return (
        <ReviewSession
          queue={queue.data ?? []}
          onClose={() => {
            popOverlay();
            bump();
          }}
          onSubmitted={bump}
        />
      );
    }
    return detailOverlay;
  })();

  // 句子加载失败 / 词典里没有该句时的可见说明；成功时不占用位置。
  const pendingSentenceNotice =
    sentenceResolveError ??
    (pendingSentenceId && !sentenceStudy.loading
      ? sentenceStudy.error ??
        (sentenceStudy.data === null
          ? "词典里没有找到这句话的记录，无法拆解。原文仍可在句库里查看。"
          : null)
      : null);

  return (
    <div className="lang-page">
      <header className="lang-header">
        <div className="lang-title">
          <Translate size={18} />
          <h1>Language</h1>
          {hasData ? null : (
            <span className="lang-muted">
              词库还是空的，先装一个 Starter Pack 就能离线学习。
            </span>
          )}
        </div>
        <div className="lang-tabs" role="tablist">
          <select
            className="lang-select"
            value={language}
            onChange={(event) => {
              setLanguage(event.target.value as LanguageCode);
              setTab("home");
            }}
            aria-label="选择语言"
          >
            {LANGUAGE_CODES.map((code) => (
              <option key={code} value={code}>
                {LANGUAGE_LABELS[code]}
              </option>
            ))}
          </select>
          {(
            [
              ["study", "学习"],
              ["home", "首页"],
              ["explore", "搜索"],
              ["review", "复习"],
              ["mistakes", "错题"],
              ["sentences", "句库"],
              ["lessons", "课程"],
              ["sources", "词库来源"],
            ] as Array<[LanguageTab, string]>
          ).map(([key, label]) => (
            <button
              key={key}
              type="button"
              role="tab"
              aria-selected={tab === key}
              className={tab === key ? "active" : ""}
              onClick={() => setTab(key)}
            >
              {label}
            </button>
          ))}
        </div>
      </header>

      <div className="lang-panel">
        {tab === "study" ? (
          <StudyCardSession
            language={language}
            onAskAi={onAskAi}
            onOpenDetail={(entityId) =>
              pushOverlay({ kind: "detail", itemId: entityId })
            }
          />
        ) : null}

        {tab === "home" ? (
          <LanguageHome
            onOpenLesson={(lessonId) => pushOverlay({ kind: "lesson", lessonId })}
            onStudyItem={(entityId) => setPendingItemId(entityId)}
            onStartReview={() => pushOverlay({ kind: "review" })}
            onOpenMistakes={() => setTab("mistakes")}
            refreshToken={refreshToken}
          />
        ) : null}

        {tab === "explore" ? (
          <>
            <ExplorePanel
              language={language}
              searchNonce={refreshToken}
              onOpenItem={(itemId) => pushOverlay({ kind: "detail", itemId })}
              onAddToLesson={(itemId, text) =>
                setLessonDraft((previous) =>
                  previous.some((entry) => entry.id === itemId)
                    ? previous
                    : [...previous, { id: itemId, text }],
                )
              }
            />
            {lessonDraft.length > 0 ? (
              <LessonDraft
                draft={lessonDraft}
                title={lessonTitle}
                onTitle={setLessonTitle}
                onRemove={(id) =>
                  setLessonDraft((previous) => previous.filter((e) => e.id !== id))
                }
                onCreate={() => void createLesson()}
                busy={creatingLesson}
              />
            ) : null}
          </>
        ) : null}

        {tab === "review" ? (
          <Panel
            title="复习"
            hint="到期 / 逾期 / 新卡"
            actions={
              <Action onClick={() => pushOverlay({ kind: "review" })}>
                开始复习
              </Action>
            }
          >
            <PanelBody
              loading={queue.loading}
              error={queue.error}
              reload={queue.reload}
              empty={!queue.loading && (queue.data?.length ?? 0) === 0 ? EMPTY_COPY.review : null}
            >
              <ReviewOverview queue={queue.data ?? []} />
            </PanelBody>
          </Panel>
        ) : null}

        {tab === "mistakes" ? (
          <Panel title="错题本" hint="答错过的内容">
            <PanelBody
              loading={mistakes.loading}
              error={mistakes.error}
              reload={mistakes.reload}
              empty={mistakes.data?.length === 0 ? EMPTY_COPY.mistakes : null}
            >
              {/* 「答对了」必须走 `submitReview`：后端在一次正确的复习提交里清除错题，
                  前端不自行删除记录。 */}
              <MistakesPanel
                mistakes={mistakes.data ?? []}
                queue={queue.data ?? []}
                loading={mistakes.loading}
                error={null}
                reload={mistakes.reload}
                onResolved={() => {
                  mistakes.reload();
                  queue.reload();
                }}
              />
            </PanelBody>
          </Panel>
        ) : null}

        {tab === "sentences" ? (
          <Panel title="句库" hint="真实语料，可逐句学习">
            <PanelBody
              loading={sentences.loading}
              error={sentences.error}
              reload={sentences.reload}
              empty={sentences.data?.length === 0 ? EMPTY_COPY.sentences : null}
              skeletonRows={5}
            >
              <ul className="lang-hit-list">
                {(sentences.data ?? []).map((record) => (
                  <li key={record.sentence_id} className="lang-hit">
                    <button
                      type="button"
                      className="lang-hit-text"
                      onClick={() => openSentenceById(record.sentence_id)}
                    >
                      <b>{record.text}</b>
                      <small>
                        {record.author ?? record.source} · {record.license}
                      </small>
                    </button>
                  </li>
                ))}
              </ul>
            </PanelBody>
          </Panel>
        ) : null}

        {tab === "lessons" ? (
          <Panel title="课程" hint="按顺序学，中途可退出">
            <PanelBody
              loading={lessons.loading}
              error={lessons.error}
              reload={lessons.reload}
              empty={lessons.data?.length === 0 ? EMPTY_COPY.lessons : null}
            >
              <ul className="lang-continue-list">
                {(lessons.data ?? []).map((lesson) => (
                  <li key={lesson.id}>
                    <div className="lang-continue-main">
                      <b>{lesson.title}</b>
                      <span className="lang-muted">
                        {lesson.steps.length} 步 · {formatStamp(lesson.updated_at)}
                      </span>
                    </div>
                    <div className="lang-row-actions">
                      <button
                        type="button"
                        className="lang-primary"
                        onClick={() => pushOverlay({ kind: "lesson", lessonId: lesson.id })}
                      >
                        打开
                      </button>
                      <button
                        type="button"
                        className="lang-danger"
                        onClick={() => void removeLesson(lesson)}
                      >
                        删除
                      </button>
                    </div>
                  </li>
                ))}
              </ul>
            </PanelBody>
          </Panel>
        ) : null}

        {tab === "sources" ? (
          <Panel
            title="词库来源"
            hint="来源、版本与许可证"
            actions={
              <Action onClick={() => void installStarter()} disabled={installing}>
                <DownloadSimple size={14} />
                {installing ? "安装中…" : "安装 Starter Pack"}
              </Action>
            }
          >
            <PanelBody
              loading={sources.loading}
              error={sources.error}
              reload={sources.reload}
              empty={sources.data?.length === 0 ? EMPTY_COPY.sources : null}
            >
              <ul className="lang-sources">
                {(sources.data ?? []).map((info) => (
                  <li key={info.source.id}>
                    <div className="lang-source-row">
                      <b>{info.source.name}</b>
                      <span>{info.item_count} 条</span>
                      <small>
                        {info.source.license.kind} · v
                        {info.source.dataset_version}
                        {info.source.license.attribution_required ? " · 需署名" : ""}
                      </small>
                    </div>
                    {info.source.notes ? (
                      <p className="lang-muted">{info.source.notes}</p>
                    ) : null}
                    {info.manifest ? (
                      <p className="lang-muted">
                        数据集 {info.manifest.name} · {info.manifest.record_count} 条 · 导入于{" "}
                        {formatStamp(info.manifest.imported_at)}
                      </p>
                    ) : null}
                  </li>
                ))}
              </ul>
            </PanelBody>
          </Panel>
        ) : null}

        {pendingSentenceNotice ? (
          <p className="lang-inline-error">{pendingSentenceNotice}</p>
        ) : null}
        {pendingItem.error ? (
          <p className="lang-inline-error">
            {pendingItem.error}
            <button type="button" className="lang-link" onClick={pendingItem.reload}>
              重试
            </button>
          </p>
        ) : null}
      </div>

      {overlayNode}
    </div>
  );
}

/** 焦点模式下当前内容的语言，供「文章内点句」搜索用。 */
function currentLanguage(target: StudyTarget): LanguageCode {
  return target.kind === "sentence" ? target.sentence.language : target.item.language;
}

/** 复习 Tab 的队列概览（不进入会话也能看清每张卡的状态）。 */
function ReviewOverview({ queue }: { queue: ReviewQueueItem[] }) {
  const summary = useMemo(
    () => ({
      due: queue.length,
      overdue: queue.filter((row) => row.is_overdue).length,
      fresh: queue.filter((row) => row.card.repetition_count === 0).length,
    }),
    [queue],
  );
  return (
    <>
      <ul className="lang-review-breakdown">
        <li>
          <Chip tone="accent">到期 {summary.due}</Chip>
        </li>
        <li>
          <Chip tone="danger">逾期 {summary.overdue}</Chip>
        </li>
        <li>
          <Chip>新卡 {summary.fresh}</Chip>
        </li>
      </ul>
      <ul className="lang-queue-preview">
        {queue.map((entry) => (
          <li key={entry.card.id}>
            <span>{entry.card.prompt}</span>
            <Chip tone={entry.is_overdue ? "danger" : "plain"}>
              {entry.card.card_type} · 复习 {entry.card.repetition_count} 次
            </Chip>
          </li>
        ))}
      </ul>
    </>
  );
}

/** 课程草稿：搜索页勾选后在这里命名并创建。 */
function LessonDraft({
  draft,
  title,
  onTitle,
  onRemove,
  onCreate,
  busy,
}: {
  draft: ReadonlyArray<{ id: string; text: string }>;
  title: string;
  onTitle: (value: string) => void;
  onRemove: (id: string) => void;
  onCreate: () => void;
  busy: boolean;
}) {
  return (
    <Panel title="课程草稿" hint={`${draft.length} 条`}>
      <div className="lang-field">
        <input
          value={title}
          onChange={(event) => onTitle(event.target.value)}
          placeholder="课程标题（留空自动生成）"
        />
      </div>
      <ul className="lang-hit-list">
        {draft.map((entry) => (
          <li key={entry.id} className="lang-hit">
            <span className="lang-hit-text">
              <b>{entry.text}</b>
            </span>
            <button type="button" className="lang-link" onClick={() => onRemove(entry.id)}>
              移除
            </button>
          </li>
        ))}
      </ul>
      <Action onClick={onCreate} disabled={busy || draft.length === 0}>
        {busy ? "创建中…" : "创建课程"}
      </Action>
    </Panel>
  );
}