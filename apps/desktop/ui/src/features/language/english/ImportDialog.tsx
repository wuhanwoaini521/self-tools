/**
 * 学习资料准备：教材（NCE）与词典（ECDICT）。
 *
 * 这一屏要回答用户的三个问题，而不是让用户猜：
 * 1. **我的数据现在在哪？** —— 显示已导入教材的来源目录与落地目录（可复制）。
 * 2. **还缺什么？** —— 明确区分「教材未导入」「词典未导入」「教材只有样本」。
 * 3. **怎么补齐？** —— 给出具体来源与操作步骤，点按钮即可选择本地路径导入。
 *
 * 原则：只读状态永远先显示；写操作（导入）显式触发，可取消，有进度。
 */
import { useCallback, useEffect, useState } from "react";
import {
  CheckCircle,
  CircleNotch,
  Copy,
  FileArrowDown,
  FolderOpen,
  Warning,
  X,
} from "@phosphor-icons/react";
import type {
  DataStatus,
  DictImportReport,
  NceImportReport,
  NceScanReport,
} from "../../../types";
import { errorMessage } from "../../../utils";
import { defaultTransport } from "../../../transport";
import { englishClient } from "./englishClient";
import { cx } from "../languageUi";

export interface ImportDialogProps {
  onClose: () => void;
  onImported: () => void;
}

type Phase = "idle" | "scanning" | "importing" | "done";

/** 教材是否明显只是样本（少于 60 课）。 */
const SAMPLE_THRESHOLD = 60;

export function ImportDialog({ onClose, onImported }: ImportDialogProps) {
  const [phase, setPhase] = useState<Phase>("idle");
  const [status, setStatus] = useState<DataStatus | null>(null);
  const [scan, setScan] = useState<NceScanReport | null>(null);
  const [report, setReport] = useState<NceImportReport | null>(null);
  const [dictReport, setDictReport] = useState<DictImportReport | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [progress, setProgress] = useState<{ done: number; total: number; message: string } | null>(
    null,
  );
  const [sourceDir, setSourceDir] = useState("");
  const [copied, setCopied] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      setStatus(await englishClient.dataStatus());
    } catch {
      // 网页端没有 data-status 命令；界面据此降级为「请在桌面端查看」。
      setStatus(null);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    return defaultTransport.subscribe<{ done: number; total: number; message: string }>(
      "language-nce-progress",
      (payload) => setProgress(payload),
    );
  }, []);

  const copy = useCallback((label: string, value: string) => {
    void navigator.clipboard
      .writeText(value)
      .then(() => {
        setCopied(label);
        window.setTimeout(() => setCopied(null), 1500);
      })
      .catch(() => setNotice("复制失败，请手动选中路径复制"));
  }, []);

  const pickFolder = useCallback(async () => {
    setError(null);
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const selected = await open({
        directory: true,
        multiple: false,
        title: "选择新概念英语文件夹（里面应该有 NCE1…NCE4）",
      });
      if (typeof selected === "string") {
        setSourceDir(selected);
        setScan(null);
      }
    } catch {
      setError("无法打开文件夹选择器。请在 self-tools 桌面应用里导入（浏览器端不支持）。");
    }
  }, []);

  const pickCsv = useCallback(async () => {
    setError(null);
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const selected = await open({
        multiple: false,
        filters: [{ name: "ECDICT 词库 csv", extensions: ["csv"] }],
        title: "选择 ECDICT 词库文件（ecdict.csv）",
      });
      if (typeof selected !== "string") return;
      setPhase("importing");
      setProgress(null);
      try {
        const result = await englishClient.dictImport(selected);
        setDictReport(result);
        await refresh();
      } catch (cause) {
        setError(errorMessage(cause));
      } finally {
        setPhase("idle");
      }
    } catch {
      setError("无法打开文件选择器。请在 self-tools 桌面应用里导入。");
    }
  }, [refresh]);

  const runScan = useCallback(async () => {
    if (!sourceDir) return;
    setPhase("scanning");
    setError(null);
    try {
      setScan(await englishClient.nceScan(sourceDir));
      setPhase("idle");
    } catch (cause) {
      setError(errorMessage(cause));
      setPhase("idle");
    }
  }, [sourceDir]);

  const runImport = useCallback(async () => {
    if (!sourceDir) return;
    setPhase("importing");
    setError(null);
    try {
      const result = await englishClient.nceImport(sourceDir);
      setReport(result);
      setPhase("done");
      await refresh();
      onImported();
    } catch (cause) {
      setError(errorMessage(cause));
      setPhase("idle");
    } finally {
      setProgress(null);
    }
  }, [onImported, refresh, sourceDir]);

  const cancelImport = useCallback(async () => {
    await englishClient.nceCancel();
  }, []);

  const missingAudio =
    scan?.books
      .flatMap((book) => book.lessons.filter((lesson) => !lesson.has_audio))
      .length ?? 0;
  const missingLrc =
    scan?.books.flatMap((book) => book.lessons.filter((lesson) => !lesson.has_lrc)).length ?? 0;
  const hasText = status !== null;
  const nceReady = (status?.nce_lessons ?? 0) > 0;
  const isSample = nceReady && (status?.nce_lessons ?? 0) < SAMPLE_THRESHOLD;

  return (
    <div className="en-modal-layer" onClick={onClose}>
      <div
        className="en-modal en-import"
        onClick={(event) => event.stopPropagation()}
        role="dialog"
        aria-label="学习资料"
      >
        <header className="en-modal-head">
          <h2>学习资料</h2>
          <button type="button" className="en-icon-btn" onClick={onClose} aria-label="关闭">
            <X size={15} />
          </button>
        </header>

        {/* ================= 现状：你的数据在哪 ================= */}
        {hasText ? (
          <section className="en-import-section is-status">
            <h3>当前数据</h3>
            <dl className="en-data-rows">
              <div className={cx("en-data-row", !nceReady && "is-missing")}>
                <dt>教材</dt>
                <dd>
                  {nceReady
                    ? `${status?.nce_books} 册 · ${status?.nce_lessons} 课（${status?.nce_lessons_with_audio} 课有音频）`
                    : "未导入"}
                </dd>
              </div>
              <div className="en-data-row">
                <dt>词典</dt>
                <dd>
                  {(status?.dict_entries ?? 0) > 0
                    ? `${(status?.dict_entries ?? 0).toLocaleString()} 个词条`
                    : "未导入"}
                </dd>
              </div>
              {status?.nce_source ? (
                <div className="en-data-row">
                  <dt>教材来源目录</dt>
                  <dd className="is-path">
                    <code>{status.nce_source}</code>
                    <button
                      type="button"
                      className="en-icon-btn"
                      onClick={() => copy("来源", status.nce_source ?? "")}
                      title="复制路径"
                    >
                      {copied === "来源" ? <CheckCircle size={13} /> : <Copy size={13} />}
                    </button>
                  </dd>
                </div>
              ) : null}
              <div className="en-data-row">
                <dt>音频存放位置</dt>
                <dd className="is-path">
                  <code>{status?.data_dir}</code>
                  <button
                    type="button"
                    className="en-icon-btn"
                    onClick={() => copy("落地", status?.data_dir ?? "")}
                    title="复制路径"
                  >
                    {copied === "落地" ? <CheckCircle size={13} /> : <Copy size={13} />}
                  </button>
                </dd>
              </div>
            </dl>
            {isSample ? (
              <p className="en-warning">
                <Warning size={14} />
                当前只有 {status?.nce_lessons} 课，属于样本。要学完整的新概念（共 276 课），
                需要重新导入一个包含 NCE1–NCE4 全部课时的文件夹。
              </p>
            ) : null}
          </section>
        ) : (
          <p className="en-muted">
            数据位置信息只能在桌面应用里查看（浏览器端无法读取本机目录）。
          </p>
        )}

        {/* ================= 教材 ================= */}
        <section className="en-import-section">
          <h3>新概念英语教材（NCE1–4）</h3>
          <p className="en-muted">
            需要一个属于你自己的新概念英语文件夹：里面按册分目录，每课一对同名文件——
            <code>.lrc</code>（带时间轴的课文与中文）和 <code>.mp3</code>（音频）。
          </p>
          <pre className="en-folder-shape">{`我的NCE/
├── NCE1/
│   ├── 001&002－Excuse Me.lrc
│   └── 001&002－Excuse Me.mp3
├── NCE2/
│   └── ……
└── NCE3/  NCE4/  …`}</pre>
          <p className="en-muted">
            没有这些文件？见下方「怎么拿到教材」。导入过的音频会复制到本地数据目录，
            之后即使移动源文件也能继续学习。
          </p>

          <div className="en-import-path">
            <button type="button" className="en-ghost-btn" onClick={() => void pickFolder()}>
              <FolderOpen size={14} /> 选择教材文件夹
            </button>
            {sourceDir ? (
              <code className="is-selected">{sourceDir}</code>
            ) : (
              <span className="en-muted">尚未选择</span>
            )}
          </div>

          {scan ? (
            <div className="en-scan-result">
              <p className="en-muted">
                找到 {scan.books.length} 册 / {scan.total_lessons} 课
                {missingAudio > 0 ? ` · ${missingAudio} 课缺音频（听力不可用）` : ""}
                {missingLrc > 0 ? ` · ${missingLrc} 课缺字幕（会跳过）` : ""}
              </p>
              {scan.books.map((book) => (
                <p key={book.book_no} className="en-scan-book">
                  NCE{book.book_no} · {book.lessons.length} 课
                </p>
              ))}
              {scan.issues.length > 0 ? (
                <p className="en-muted">提示：{scan.issues.slice(0, 2).join("；")}</p>
              ) : null}
            </div>
          ) : null}

          {progress ? (
            <div className="en-import-progress">
              <div className="en-progress-track">
                <i
                  style={{
                    width: `${Math.round((progress.done / Math.max(1, progress.total)) * 100)}%`,
                  }}
                />
              </div>
              <p className="en-muted">
                {progress.done}/{progress.total} · {progress.message}
              </p>
            </div>
          ) : null}

          {report ? (
            <div className="en-import-report">
              <p>
                <CheckCircle size={14} /> 导入完成：{report.books} 册 / {report.lessons} 课 /{" "}
                {report.sentences} 句 / {report.vocab} 个生词
                {report.cancelled ? "（已取消）" : ""}
              </p>
              {report.issues.length > 0 ? (
                <details>
                  <summary>{report.issues.length} 条提示</summary>
                  <ul className="en-issue-list">
                    {report.issues.slice(0, 12).map((issue, index) => (
                      <li key={index}>{issue}</li>
                    ))}
                  </ul>
                </details>
              ) : null}
            </div>
          ) : null}

          {sourceDir ? (
            <div className="en-row-actions">
              <button
                type="button"
                className="en-ghost-btn"
                onClick={() => void runScan()}
                disabled={phase !== "idle"}
              >
                {phase === "scanning" ? (
                  <>
                    <CircleNotch size={14} className="is-spin" /> 扫描中…
                  </>
                ) : (
                  "先扫描看看"
                )}
              </button>
              <button
                type="button"
                className="en-primary-btn"
                onClick={() => void runImport()}
                disabled={phase !== "idle"}
              >
                确认导入
              </button>
            </div>
          ) : null}
        </section>

        {/* ================= 词典 ================= */}
        <section className="en-import-section">
          <h3>ECDICT 英汉词典（强烈建议）</h3>
          <p className="en-muted">
            {hasText && (status?.dict_entries ?? 0) > 0
              ? `已导入 ${(status?.dict_entries ?? 0).toLocaleString()} 个词条，生词已有音标与中文释义。`
              : "未导入。没有词典也能学，但生词卡只有英文，没有音标和中文释义。"}
          </p>
          <p className="en-muted">
            需要 ECDICT 的 <code>ecdict.csv</code>（约 77 万词条，含音标、中文释义、词频与词形变化）。
            获取方式见文档；导入一次约需 10 秒。
          </p>
          {dictReport ? (
            <p className="en-muted">
              本次导入 {dictReport.entries.toLocaleString()} 条
              {dictReport.cancelled ? "（已取消）" : ""}
            </p>
          ) : null}
          <div>
            <button
              type="button"
              className="en-ghost-btn"
              onClick={() => void pickCsv()}
              disabled={phase !== "idle"}
            >
              <FileArrowDown size={14} /> 选择 ecdict.csv
            </button>
          </div>
        </section>

        {/* ================= 怎么拿到数据 ================= */}
        <section className="en-import-section is-help">
          <h3>怎么拿到教材和词典？</h3>
          <ol className="en-help-list">
            <li>
              <b>词典</b>：ECDICT 是开源词库，
              <code>raw.githubusercontent.com/skywind3000/ECDICT/master/ecdict.csv</code>
               下载 <code>ecdict.csv</code> 即可。
            </li>
            <li>
              <b>教材</b>：新概念英语本身有版权，self-tools 不内置也不代为分发。
              需要你自备带 <code>.lrc</code> + <code>.mp3</code> 的本地文件
              （网上常见的新概念 MP3 同步字幕版即可）。
            </li>
            <li>
              <b>本机已准备的示例</b>：本仓库目录下 <code>config/nce-sample/</code>（6 课）与
              <code>config/ecdict.csv</code>（完整词典）可直接用于试跑。
            </li>
          </ol>
        </section>

        {error ? <p className="en-inline-error">{error}</p> : null}
        {notice ? <p className="en-muted">{notice}</p> : null}

        <footer className="en-modal-foot">
          {phase === "importing" ? (
            <button type="button" className="en-danger-btn" onClick={() => void cancelImport()}>
              取消导入
            </button>
          ) : (
            <button type="button" className="en-ghost-btn" onClick={onClose}>
              关闭
            </button>
          )}
        </footer>
      </div>
    </div>
  );
}