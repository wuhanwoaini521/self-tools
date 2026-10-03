/**
 * 词典详情（`item(id)` → `WordDetail`）。
 *
 * 覆盖：词头 / 多个读音 / 释义 / 例句 / 相关词 / 汉字信息 / 来源与许可证。
 * 每个缺省字段都显式说明「词典未收录」，不做无中生有的补全。
 */
import {
  ArrowSquareOut,
  BookmarkSimple,
  SpeakerHigh,
  X,
} from "@phosphor-icons/react";
import { useEffect, useState } from "react";
import type { WordDetail } from "../../types";
import { languageClient } from "./languageClient";
import { errorMessage } from "../../utils";
import { learningClient } from "../learning/learningClient";
import { speak } from "./tts";
import { Action, Chip, PanelBody } from "./LanguagePrimitives";
import { cx, formatStamp, useAsyncPanel } from "./languageUi";

export interface WordDetailViewProps {
  itemId: string;
  onClose: () => void;
  onAskAi?: (prompt: string) => void;
  /** 加入复习队列后的通知（首页据此刷新）。 */
  onBookmarked?: () => void;
  /** 详情面板里的「加入课程」。 */
  onAddToLesson?: (itemId: string, content: string) => void;
}

export function WordDetailView({
  itemId,
  onClose,
  onAskAi,
  onBookmarked,
  onAddToLesson,
}: WordDetailViewProps) {
  const panel = useAsyncPanel<WordDetail | null>(
    () => languageClient.item(itemId),
    [itemId],
  );
  const [collections, setCollections] = useState<
    Array<{ id: string; title: string; items_count: number }>
  >([]);
  const [collectionId, setCollectionId] = useState<string>("");
  const [note, setNote] = useState<string>("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // 合集列表只是附赠功能：失败就隐藏这一段，不影响词典详情。
  useEffect(() => {
    let alive = true;
    learningClient
      .listCollections()
      .then((rows) => {
        if (!alive) return;
        setCollections(rows);
        setCollectionId((current) => current || (rows[0]?.id ?? ""));
      })
      .catch(() => {
        if (alive) setCollections([]);
      });
    return () => {
      alive = false;
    };
  }, []);

  const detail = panel.data;
  const item = detail?.item ?? null;

  const addToReview = async () => {
    setBusy(true);
    setError(null);
    try {
      await languageClient.addToReview(itemId);
      onBookmarked?.();
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setBusy(false);
    }
  };

  const addToCollection = async () => {
    if (!item || !collectionId) return;
    setBusy(true);
    setError(null);
    try {
      await languageClient.addToCollection(
        collectionId,
        itemId,
        note.trim() ? note.trim() : undefined,
      );
      setNote("");
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="lang-detail-overlay" role="dialog" aria-modal="true">
      <div className="lang-detail">
        <header className="lang-detail-head">
          <span>词典详情</span>
          <button type="button" onClick={onClose} title="关闭 (Esc)">
            <X size={16} />
          </button>
        </header>

        <PanelBody
          loading={panel.loading}
          error={panel.error}
          reload={panel.reload}
          empty={!panel.loading && !detail ? "词典里没有这条内容。" : null}
          skeletonRows={5}
        >
          {detail && item ? (
            <div className="lang-detail-body">
              <header className="lang-word-head">
                <h2>{item.text}</h2>
                {item.reading ? (
                  <span className="lang-reading">{item.reading}</span>
                ) : null}
                {item.romanization ? (
                  <p className="lang-roman">{item.romanization}</p>
                ) : null}
                <div className="lang-chip-row">
                  <Chip tone="accent">{item.item_type}</Chip>
                  <Chip>{item.language}</Chip>
                  <Chip>{item.source}</Chip>
                </div>
                <div className="lang-row-actions">
                  <button
                    type="button"
                    className="lang-link"
                    onClick={() => void speak(item.text, item.language)}
                  >
                    <SpeakerHigh size={14} /> 朗读
                  </button>
                  <Action
                    variant="ghost"
                    onClick={() => void addToReview()}
                    disabled={busy}
                  >
                    <BookmarkSimple size={14} /> 加入复习
                  </Action>
                  {onAddToLesson ? (
                    <button
                      type="button"
                      className="lang-link"
                      onClick={() => onAddToLesson(item.id, item.text)}
                    >
                      加入课程
                    </button>
                  ) : null}
                  {onAskAi ? (
                    <button
                      type="button"
                      className="lang-link"
                      onClick={() => onAskAi(`请讲解「${item.text}」的用法与例句。`)}
                    >
                      问 AI
                    </button>
                  ) : null}
                </div>
              </header>

              {detail.pronunciations.length > 0 ? (
                <section>
                  <h3>发音</h3>
                  <ul className="lang-prons">
                    {detail.pronunciations.map((pron) => (
                      <li key={pron.id}>
                        <b>{pron.phonemes}</b>
                        <small>
                          {pron.scheme}
                          {pron.variant ? ` · ${pron.variant}` : ""}
                          {pron.tone !== null ? ` · 声调 ${pron.tone}` : ""}
                        </small>
                      </li>
                    ))}
                  </ul>
                </section>
              ) : null}

              {detail.meanings.length > 0 ? (
                <section>
                  <h3>释义</h3>
                  <ul className="lang-meanings">
                    {detail.meanings.map((meaning) => (
                      <li key={meaning.id}>
                        {meaning.pos ? (
                          <span className="lang-pos">{meaning.pos}</span>
                        ) : null}
                        <span>{meaning.gloss ?? meaning.raw ?? "（未收录释义）"}</span>
                        <small>{meaning.source}</small>
                      </li>
                    ))}
                  </ul>
                </section>
              ) : (
                <section>
                  <h3>释义</h3>
                  <p className="lang-muted">词典未收录这个词的释义。</p>
                </section>
              )}

              {detail.kanji ? (
                <section>
                  <h3>汉字</h3>
                  <ul className="lang-kanji">
                    <li>
                      <b>读音</b> {detail.kanji.readings.join("、") || "—"}
                    </li>
                    <li>
                      <b>意义</b> {detail.kanji.meanings.join("、") || "—"}
                    </li>
                    <li>
                      <b>笔画</b> {detail.kanji.stroke_count ?? "—"}
                      {detail.kanji.grade !== null ? ` · 学级 ${detail.kanji.grade}` : ""}
                      {detail.kanji.jlpt !== null ? ` · JLPT N${detail.kanji.jlpt}` : ""}
                    </li>
                  </ul>
                </section>
              ) : null}

              {detail.examples.length > 0 ? (
                <section>
                  <h3>例句</h3>
                  <ul className="lang-examples">
                    {detail.examples.map((example, index) => (
                      <li key={`${example.text}-${index}`} className="lang-sentence">
                        <button
                          type="button"
                          onClick={() => void speak(example.text, item.language)}
                        >
                          <span>{example.text}</span>
                        </button>
                        {example.translation ? (
                          <small>{example.translation}</small>
                        ) : null}
                        <small>{example.source}</small>
                      </li>
                    ))}
                  </ul>
                </section>
              ) : null}

              {detail.sentences.length > 0 ? (
                <section>
                  <h3>真实语料</h3>
                  <ul className="lang-examples">
                    {detail.sentences.map((record) => (
                      <li key={record.sentence_id} className="lang-sentence">
                        <span>{record.text}</span>
                        <small>
                          {record.author ?? record.source} · {record.license}
                        </small>
                      </li>
                    ))}
                  </ul>
                </section>
              ) : null}

              {detail.relations.length > 0 ? (
                <section>
                  <h3>相关词</h3>
                  <ul className="lang-related">
                    {detail.relations.map((relation) => (
                      <li key={relation.relation.id}>
                        <i>{relation.label}</i>
                        <button type="button">
                          {relation.item.text}
                        </button>
                      </li>
                    ))}
                  </ul>
                </section>
              ) : null}

              <section className="lang-source">
                {detail.source ? (
                  <>
                    <b>{detail.source.name}</b>
                    <small>
                      v{detail.source.dataset_version} ·{" "}
                      {detail.source.license.kind}
                      {detail.source.license.attribution_required
                        ? " · 需署名"
                        : ""}
                      {detail.source.license.commercial_use ? " · 可商用" : ""}
                      {detail.source.license.redistribution ? " · 可再分发" : ""}
                    </small>
                    <small>
                      下载于 {formatStamp(detail.source.downloaded_at)}
                    </small>
                    {detail.source.attribution ? (
                      <small>署名要求：{detail.source.attribution}</small>
                    ) : null}
                    <a
                      className="lang-link"
                      href={detail.source.license_url ?? detail.source.homepage}
                      target="_blank"
                      rel="noreferrer noopener"
                    >
                      <ArrowSquareOut size={13} /> 许可证 / 主页
                    </a>
                  </>
                ) : (
                  <small>这条内容没有登记来源信息。</small>
                )}
              </section>

              {collections.length > 0 ? (
                <section className="lang-sources-settings">
                  <h3>加入合集</h3>
                  <div className="lang-field">
                    <select
                      value={collectionId}
                      onChange={(event) => setCollectionId(event.target.value)}
                      aria-label="选择合集"
                    >
                      {collections.map((collection) => (
                        <option key={collection.id} value={collection.id}>
                          {collection.title}（{collection.items_count}）
                        </option>
                      ))}
                    </select>
                  </div>
                  <div className="lang-field">
                    <input
                      value={note}
                      onChange={(event) => setNote(event.target.value)}
                      placeholder="备注（可选）"
                    />
                  </div>
                  <Action
                    onClick={() => void addToCollection()}
                    disabled={busy || !collectionId}
                  >
                    加入合集
                  </Action>
                </section>
              ) : (
                <p className="lang-muted">
                  还没有合集。到「学习 OS → 合集」里建一个，就能把这条内容收进去。
                </p>
              )}

              {error ? (
                <p className={cx("lang-inline-error-text")}>{error}</p>
              ) : null}
            </div>
          ) : null}
        </PanelBody>
      </div>
    </div>
  );
}