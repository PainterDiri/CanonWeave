import { useCallback, useEffect, useState } from "react";
import { Bot, LoaderCircle, Plus, Trash2 } from "lucide-react";
import { Button, Field, Input, Textarea } from "@/components/ui";
import { ipc, type CraftReport, type StoryThread, type StoryThreadInput } from "@/lib/ipc";
import { useAppStore } from "@/stores/ui";

const threadKinds = [
  { key: "milieu", label: "环境线", hint: "进入一个新世界，离开时关闭" },
  { key: "inquiry", label: "疑问线", hint: "提出一个问题，解答时关闭" },
  { key: "character", label: "人物线", hint: "人物对自身不满，改变或放弃时关闭" },
  { key: "event", label: "事件线", hint: "秩序被打破，恢复或建立新秩序时关闭" },
];

const emptyThread: StoryThreadInput = {
  id: null,
  kind: "inquiry",
  title: "",
  question: "",
  openChapter: null,
  closeChapter: null,
  status: "open",
  notes: "",
};

function kindLabel(kind: string) {
  return threadKinds.find((item) => item.key === kind)?.label ?? kind;
}

function numberOrNull(value: string): number | null {
  const parsed = Number.parseInt(value, 10);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : null;
}

/** 章节工艺诊断 + 叙事线程台账。线程按“先开后关”嵌套，交错收束会被标出。 */
export function CraftPanel() {
  const setNav = useAppStore((s) => s.setNav);
  const setAgentDraft = useAppStore((s) => s.setAgentDraft);
  const notify = useAppStore((s) => s.notify);
  const [report, setReport] = useState<CraftReport | null>(null);
  const [threads, setThreads] = useState<StoryThread[]>([]);
  const [draft, setDraft] = useState<StoryThreadInput>(emptyThread);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    const [nextReport, nextThreads] = await Promise.all([ipc.craftReport(), ipc.storyThreads()]);
    setReport(nextReport);
    setThreads(nextThreads ?? []);
  }, []);

  useEffect(() => {
    void load()
      .catch((cause) => setError(cause instanceof Error ? cause.message : String(cause)))
      .finally(() => setLoading(false));
  }, [load]);

  const save = async () => {
    if (!draft.title.trim()) {
      setError("先写下这条线程的名字。");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await ipc.storyThreadSave({
        ...draft,
        status: draft.closeChapter ? "closed" : draft.status === "dropped" ? "dropped" : "open",
      });
      setDraft(emptyThread);
      await load();
      notify("叙事线程已保存");
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setBusy(false);
    }
  };

  const remove = async (thread: StoryThread) => {
    if (!window.confirm(`删除叙事线程「${thread.title}」？这不会影响章节或正文。`)) return;
    try {
      await ipc.storyThreadDelete(thread.id);
      await load();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    }
  };

  const delegate = (prompt: string) => {
    setAgentDraft(prompt);
    setNav("agent");
  };

  if (loading) {
    return (
      <div className="flex h-48 items-center justify-center text-[12px] text-muted">
        <LoaderCircle size={14} className="mr-2 animate-spin" /> 正在诊断章节工艺
      </div>
    );
  }

  const maxChapter = Math.max(
    report?.chapterCount ?? 0,
    ...threads.map((thread) => thread.closeChapter ?? thread.openChapter ?? 0),
    1,
  );
  const threadChecks = new Map<string, string[]>();
  for (const check of report?.checks ?? []) {
    if (!check.threadId) continue;
    threadChecks.set(check.threadId, [...(threadChecks.get(check.threadId) ?? []), check.message]);
  }
  const chapterChecks = (report?.checks ?? []).filter((check) => !check.threadId);

  return (
    <div className="grid grid-cols-[minmax(0,1.25fr)_minmax(300px,1fr)] gap-5">
      <section className="paper-card p-5">
        <div className="mb-1 flex items-start justify-between gap-3">
          <div>
            <h2 className="display text-[19px] text-ink">叙事线程</h2>
            <p className="mt-1 text-[11.5px] leading-5 text-muted">
              每条线向读者提出一个问题。线程应层层嵌套：后开启的先收束。
            </p>
          </div>
          <Button
            size="sm"
            icon={<Bot size={13} />}
            onClick={() =>
              delegate("读取大纲和已有正文，梳理环境、疑问、人物、事件四类叙事线程，标出开启与收束章节，并提出线程候选。")
            }
          >
            让 Agent 梳理
          </Button>
        </div>

        {threads.length === 0 ? (
          <p className="my-6 text-center text-[12px] text-muted">
            还没有线程。先记下全书最大的那个问题，它通常是最外层的线。
          </p>
        ) : (
          <ul className="mt-4 space-y-2.5">
            {threads.map((thread) => {
              const open = thread.openChapter ?? 1;
              const close = thread.closeChapter ?? maxChapter;
              const left = ((open - 1) / maxChapter) * 100;
              const width = Math.max(((close - open + 1) / maxChapter) * 100, 2);
              const warnings = threadChecks.get(thread.id) ?? [];
              return (
                <li key={thread.id} className="group">
                  <div className="flex items-baseline gap-2">
                    <button
                      className="text-left text-[12.5px] font-medium text-ink hover:text-teal"
                      onClick={() =>
                        setDraft({
                          id: thread.id,
                          kind: thread.kind,
                          title: thread.title,
                          question: thread.question,
                          openChapter: thread.openChapter,
                          closeChapter: thread.closeChapter,
                          status: thread.status,
                          notes: thread.notes,
                        })
                      }
                    >
                      {thread.title}
                    </button>
                    <span className="text-[10px] text-muted">{kindLabel(thread.kind)}</span>
                    <span className="ml-auto text-[10px] tabular-nums text-muted">
                      {thread.openChapter ? `第${thread.openChapter}章` : "?"} →{" "}
                      {thread.closeChapter ? `第${thread.closeChapter}章` : "未关闭"}
                    </span>
                    <button
                      onClick={() => void remove(thread)}
                      className="text-muted opacity-0 transition group-hover:opacity-100 hover:text-coral"
                      aria-label={`删除线程 ${thread.title}`}
                    >
                      <Trash2 size={11} />
                    </button>
                  </div>
                  <div className="relative mt-1 h-2 rounded-full bg-silk-soft/60">
                    <div
                      className={`absolute top-0 h-2 rounded-full ${
                        thread.closeChapter ? "bg-teal/80" : "bg-silk"
                      } ${warnings.length ? "ring-1 ring-amber" : ""}`}
                      style={{ left: `${left}%`, width: `${Math.min(width, 100 - left)}%` }}
                    />
                  </div>
                  {thread.question ? (
                    <div className="mt-0.5 text-[10.5px] leading-5 text-muted">{thread.question}</div>
                  ) : null}
                  {warnings.map((warning) => (
                    <div key={warning} className="text-[10.5px] leading-5 text-amber">
                      {warning}
                    </div>
                  ))}
                </li>
              );
            })}
          </ul>
        )}

        <div className="mt-5 rounded-xl border border-line bg-paper/70 p-3.5">
          <div className="mb-2.5 flex items-center justify-between text-[12px] font-medium text-ink-soft">
            {draft.id ? "编辑线程" : "新增线程"}
            {draft.id ? (
              <button className="text-[11px] text-muted hover:text-ink" onClick={() => setDraft(emptyThread)}>
                取消编辑
              </button>
            ) : null}
          </div>
          <div className="mb-2.5 flex flex-wrap gap-1.5" role="radiogroup" aria-label="线程类型">
            {threadKinds.map((kind) => (
              <button
                key={kind.key}
                role="radio"
                aria-checked={draft.kind === kind.key}
                title={kind.hint}
                onClick={() => setDraft((value) => ({ ...value, kind: kind.key }))}
                className={`rounded-full border px-2.5 py-0.5 text-[11px] transition ${
                  draft.kind === kind.key
                    ? "border-teal bg-teal text-paper-raised"
                    : "border-line bg-paper-raised text-ink-soft hover:border-teal/40"
                }`}
              >
                {kind.label}
              </button>
            ))}
          </div>
          <div className="grid grid-cols-[1fr_88px_88px] gap-2">
            <Field label="线程名">
              <Input value={draft.title} onChange={(e) => setDraft((v) => ({ ...v, title: e.target.value }))} />
            </Field>
            <Field label="开启章">
              <Input
                inputMode="numeric"
                value={draft.openChapter ?? ""}
                onChange={(e) => setDraft((v) => ({ ...v, openChapter: numberOrNull(e.target.value) }))}
              />
            </Field>
            <Field label="关闭章">
              <Input
                inputMode="numeric"
                value={draft.closeChapter ?? ""}
                onChange={(e) => setDraft((v) => ({ ...v, closeChapter: numberOrNull(e.target.value) }))}
              />
            </Field>
          </div>
          <div className="mt-2">
            <Field label="向读者提出的问题">
              <Textarea
                rows={2}
                value={draft.question}
                onChange={(e) => setDraft((v) => ({ ...v, question: e.target.value }))}
              />
            </Field>
          </div>
          {error ? <div className="mt-2 text-[11px] text-coral">{error}</div> : null}
          <div className="mt-3 flex justify-end">
            <Button tone="primary" size="sm" icon={<Plus size={13} />} onClick={() => void save()} disabled={busy}>
              {draft.id ? "保存线程" : "添加线程"}
            </Button>
          </div>
        </div>
      </section>

      <section className="paper-card p-5">
        <h2 className="display text-[19px] text-ink">章节工艺诊断</h2>
        <p className="mt-1 text-[11.5px] leading-5 text-muted">
          每章都要有价值转变与两难抉择；非末章要有具体的章末钩子。诊断只提示，不改动内容。
        </p>
        {report ? (
          <div className="mt-4 grid grid-cols-3 gap-2 text-center">
            {[
              ["价值转变", `${report.chaptersWithShift}/${report.chapterCount}`],
              ["章末钩子", `${report.chaptersWithHook}/${report.chapterCount}`],
              ["未关线程", `${report.openThreads}`],
            ].map(([label, value]) => (
              <div key={label} className="rounded-lg bg-paper/80 px-2 py-2">
                <div className="display text-[18px] tabular-nums text-teal-deep">{value}</div>
                <div className="text-[10.5px] text-muted">{label}</div>
              </div>
            ))}
          </div>
        ) : null}
        <ul className="mt-4 max-h-[420px] space-y-2 overflow-y-auto pr-1">
          {chapterChecks.length === 0 ? (
            <li className="text-[12px] text-muted">没有需要处理的工艺问题。</li>
          ) : (
            chapterChecks.slice(0, 40).map((check, index) => (
              <li key={`${check.code}-${index}`} className="text-[11.5px] leading-5">
                <div className={check.severity === "warning" ? "text-amber" : "text-ink-soft"}>{check.message}</div>
                <div className="text-[10.5px] text-muted">{check.suggestion}</div>
              </li>
            ))
          )}
        </ul>
        {chapterChecks.length > 0 ? (
          <Button
            className="mt-4"
            size="sm"
            icon={<Bot size={13} />}
            onClick={() =>
              delegate("运行 craft_check，逐章读取工艺卡和因果上下文，为缺少价值转变、钩子或两难抉择的章节提出工艺卡修订候选。")
            }
          >
            让 Agent 提出修订
          </Button>
        ) : null}
      </section>
    </div>
  );
}
