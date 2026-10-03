/**
 * 数据导入对话框：NCE 教材 + ECDICT 词典。
 *
 * 这是一次性设置，不是日常功能（任务书 §6），所以放在弹窗里而不是页面入口。
 * 要求：
 * - 用系统原生文件夹/文件选择器选路径（前端 plugin-dialog，浏览器里不可用时明确说明）；
 * - 先扫描再导入：扫描结果先展示「找到几册几课 / 缺什么」，避免误导；
 * - 导入中显示进度 + 可取消；真实数据量由后端事件推送；
 * - 教材与音频不进 Git（用户本地数据目录）。
 */
import { useCallback, useEffect, useRef, useState } from "react";
import { FolderOpen, FileArrowDown, X, CircleNotch, CheckCircle } from "@phosphor-icons/react";
import type {
  DictImportReport,
  DictStatus,
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

type Phase = "idle" | "scanning" | "scanned" | "importing" | "done";

export function ImportDialog({ onClose, onImported }: ImportDialogProps) {
  const [phase, setPhase] = useState<Phase>("idle");
  const [scan, setScan] = useState<NceScanReport | null>(null);
  const [report, setReport] = useState<NceImportReport | null>(null);
  const [dictReport, setDictReport] = useState<DictImportReport | null>(null);
  const [dictStatus, setDictStatus] = useState<DictStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [progress, setProgress] = useState<{ done: number; total: number; message: string } | null>(
    null,
  );
  const [sourceDir, setSourceDir] = useState<string>("");
  const startedRef = useRef(false);

  const refreshDictStatus = useCallback(async () => {
    try {
      setDictStatus(await englishClient.dictStatus());
    } catch {
      setDictStatus(null);
    }
  }, []);

  useEffect(() => {
    void refreshDictStatus();
  }, [refreshDictStatus]);

  // 订阅后端进度事件（导入期间推送）。
  useEffect(() => {
    const dispose = defaultTransport.subscribe<{
      done: number;
      total: number;
      message: string;
    }>("language-nce-progress", (payload) => {
      setProgress(payload);
    });
    return dispose;
  }, []);

  // 关闭弹窗时中止导入，避免后台继续跑。
  useEffect(() => {
    return () => {
      if (startedRef.current) void englishClient.nceCancel();
    };
  }, []);

  const pickFolder = useCallback(async () => {
    setError(null);
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const selected = await open({ directory: true, multiple: false, title: "选择新概念英语文件夹" });
      if (typeof selected === "string") {
        setSourceDir(selected);
        setScan(null);
      }
    } catch {
      setError("无法打开文件夹选择器（当前不是桌面运行时）。请在桌面应用里导入。");
    }
  }, []);

  const pickCsv = useCallback(async () => {
    setError(null);
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const selected = await open({
        multiple: false,
        filters: [{ name: "ECDICT csv", extensions: ["csv"] }],
        title: "选择 ECDICT 词库文件（ecdict.csv）",
      });
      if (typeof selected !== "string") return;
      setPhase("importing");
      setProgress(null);
      try {
        const result = await englishClient.dictImport(selected);
        setDictReport(result);
        await refreshDictStatus();
      } catch (cause) {
        setError(errorMessage(cause));
      } finally {
        setPhase("idle");
      }
    } catch {
      setError("无法打开文件选择器（当前不是桌面运行时）。");
    }
  }, [refreshDictStatus]);

  const runScan = useCallback(async () => {
    if (!sourceDir) return;
    setPhase("scanning");
    setError(null);
    try {
      setScan(await englishClient.nceScan(sourceDir));
      setPhase("scanned");
    } catch (cause) {
      setError(errorMessage(cause));
      setPhase("idle");
    }
  }, [sourceDir]);

  const runImport = useCallback(async () => {
    if (!sourceDir) return;
    setPhase("importing");
    setError(null);
    startedRef.current = true;
    try {
      const result = await englishClient.nceImport(sourceDir);
      setReport(result);
      setPhase("done");
      onImported();
    } catch (cause) {
      setError(errorMessage(cause));
      setPhase("scanned");
    } finally {
      startedRef.current = false;
      setProgress(null);
    }
  }, [onImported, sourceDir]);

  const cancelImport = useCallback(async () => {
    await englishClient.nceCancel();
  }, []);

  const missingAudio =
    scan?.books.flatMap((book) => book.lessons.filter((lesson) => !lesson.has_audio)).length ?? 0;
  const missingLrc =
    scan?.books.flatMap((book) => book.lessons.filter((lesson) => !lesson.has_lrc)).length ?? 0;

  return (
    <div className="en-modal-layer" onClick={onClose}>
      <div className="en-modal" onClick={(event) => event.stopPropagation()} role="dialog" aria-label="导入教材与词典">
        <header className="en-modal-head">
          <h2>导入学习资料</h2>
          <button type="button" className="en-icon-btn" onClick={onClose} aria-label="关闭">
            <X size={15} />
          </button>
        </header>

        <p className="en-muted">
          教材与音频只保存在你自己的电脑上（不会上传、不会进入项目仓库），导入后即可离线学习。
        </p>

        {/* ---- 词典 ---- */}
        <section className="en-import-section">
          <h3>ECDICT 词典（可选）</h3>
          <p className="en-muted">
            {dictStatus?.ready
              ? `已导入 ${dictStatus.count.toLocaleString()} 个词条`
              : "未导入。导入后生词会有音标、中文释义与词频；不导入也能学习，只是释义缺失。"}
          </p>
          {dictReport ? (
            <p className="en-muted">
              本次导入 {dictReport.entries.toLocaleString()} 条
              {dictReport.cancelled ? "（已取消）" : ""}
            </p>
          ) : null}
          <button type="button" className="en-ghost-btn" onClick={() => void pickCsv()}>
            <FileArrowDown size={14} /> 选择 ECDICT csv
          </button>
        </section>

        {/* ---- 教材 ---- */}
        <section className="en-import-section">
          <h3>新概念英语（NCE1–NCE4）</h3>
          <p className="en-muted">
            选择包含 NCE1…NCE4 的文件夹：每课一对同名 <code>.lrc</code> + <code>.mp3</code>。
          </p>
          <div className="en-import-path">
            <button type="button" className="en-ghost-btn" onClick={() => void pickFolder()}>
              <FolderOpen size={14} /> 选择文件夹
            </button>
            <span className="en-muted">{sourceDir || "尚未选择"}</span>
          </div>

          {scan ? (
            <div className="en-scan-result">
              <p className="en-muted">
                找到 {scan.books.length} 册 / {scan.total_lessons} 课
                {missingAudio > 0 ? ` · ${missingAudio} 课缺音频` : ""}
                {missingLrc > 0 ? ` · ${missingLrc} 课缺字幕` : ""}
              </p>
              {scan.books.map((book) => (
                <p key={book.book_no} className="en-scan-book">
                  NCE{book.book_no} · {book.lessons.length} 课
                </p>
              ))}
              {scan.issues.length > 0 ? (
                <p className="en-muted">提示：{scan.issues.slice(0, 3).join("；")}</p>
              ) : null}
            </div>
          ) : null}

          {progress ? (
            <div className="en-import-progress">
              <div className="en-progress-track">
                <i style={{ width: `${Math.round((progress.done / Math.max(1, progress.total)) * 100)}%` }} />
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
        </section>

        {error ? <p className="en-inline-error">{error}</p> : null}

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
          {sourceDir && phase !== "importing" ? (
            <button
              type="button"
              className="en-primary-btn"
              onClick={() => (scan ? void runImport() : void runScan())}
            >
              {phase === "scanning" ? (
                <>
                  <CircleNotch size={14} className={cx("is-spin")} /> 扫描中…
                </>
              ) : scan ? (
                "导入这一册"
              ) : (
                "扫描文件夹"
              )}
            </button>
          ) : null}
        </footer>
      </div>
    </div>
  );
}