import { Markdown } from "@/components/Markdown";
import { useEffect, useState } from "react";
import {
  AlertCircle,
  BookOpenText,
  Check,
  ChevronDown,
  ChevronRight,
  CircleDashed,
  ClipboardList,
  FilePlus2,
  FolderTree,
  HelpCircle,
  LoaderCircle,
  Search,
  ShieldCheck,
  Sparkles,
  Workflow,
} from "lucide-react";
import { ipc, type HarnessToolTrace } from "@/lib/ipc";

export type AgentMode = "plan" | "write";

export type LiveItem =
  | { type: "text"; text: string }
  | { type: "tool"; trace: HarnessToolTrace };

export type PlanItem = { title: string; status: "pending" | "in_progress" | "done" };

export type ApprovalItem =
  | { kind: "premise"; id: string; title: string; summary: string }
  | { kind: "blueprint"; id: string; title: string; summary: string; blocked: boolean }
  | { kind: "outline"; ids: string[]; title: string; summary: string }
  | { kind: "candidate"; id: string };

export type AgentAction =
  | { type: "ask_author"; question: string; options: string[] }
  | { type: "approval"; group: string; title: string; items: ApprovalItem[] };

/** 确认卡里候选的展示信息；返回 null 表示候选已处理。 */
export type CandidateDescriber = (
  id: string,
) => { label: string; summary: string; kind: string; risk: string } | null;

function toolIcon(name: string) {
  if (name === "list_paths") return FolderTree;
  if (name === "read_path") return BookOpenText;
  if (name === "search") return Search;
  if (name === "run_audit" || name === "craft_check") return ShieldCheck;
  if (name === "update_plan") return ClipboardList;
  if (name === "ask_author") return HelpCircle;
  if (name.startsWith("generate_") || name === "recommend_style") return Workflow;
  if (name.startsWith("propose_")) return FilePlus2;
  return Sparkles;
}

/** 从工具轨迹还原需要作者处理的动作卡：提问选项 + 本轮产物的确认卡。 */
export function actionsFromTraces(traces: HarnessToolTrace[]): AgentAction[] {
  const actions: AgentAction[] = [];
  const candidateIds: string[] = [];
  for (const trace of traces) {
    if (trace.status !== "completed") continue;
    const input = trace.input ?? {};
    const action = trace.action as Record<string, any> | null;
    if (trace.toolName === "ask_author") {
      actions.push({
        type: "ask_author",
        question: String(input.question ?? trace.summary),
        options: Array.isArray(input.options) ? input.options.map(String).slice(0, 4) : [],
      });
      continue;
    }
    if (action?.type === "approval" && Array.isArray(action.items)) {
      actions.push({
        type: "approval",
        group: String(action.group ?? "items"),
        title: String(action.title ?? "确认本轮产物"),
        items: action.items as ApprovalItem[],
      });
    }
    candidateIds.push(...trace.candidateIds);
  }
  if (candidateIds.length > 0) {
    actions.push({
      type: "approval",
      group: "candidates",
      title: "确认本轮提出的候选",
      items: candidateIds.map((id) => ({ kind: "candidate", id })),
    });
  }
  // 提问放在最后，确认卡在前，作者先处理产物再回答问题。
  return [...actions.filter((a) => a.type === "approval"), ...actions.filter((a) => a.type === "ask_author")];
}

export function ToolRow({ trace }: { trace: HarnessToolTrace }) {
  const Icon = toolIcon(trace.toolName);
  const running = trace.status === "running";
  const failed = trace.status === "failed";
  const proposed = trace.candidateIds.length > 0;
  return (
    <div className="flex items-center gap-2 py-[3px] text-[10.5px] leading-5">
      <span
        className={`flex h-[18px] w-[18px] shrink-0 items-center justify-center rounded-md ${
          failed
            ? "bg-coral-soft text-coral"
            : proposed
              ? "bg-amber-soft text-amber"
              : "bg-teal-soft/70 text-teal"
        }`}
      >
        {running ? <LoaderCircle size={10} className="animate-spin" /> : <Icon size={10} />}
      </span>
      <span className={`truncate ${failed ? "text-coral" : "text-ink-soft"}`}>{trace.title}</span>
      {trace.summary ? (
        <span className="min-w-0 truncate text-muted">· {trace.summary}</span>
      ) : null}
      {proposed ? (
        <span className="ml-auto shrink-0 rounded-full bg-amber-soft px-1.5 text-[9px] text-amber">
          候选 {trace.candidateIds.length}
        </span>
      ) : null}
    </div>
  );
}

export function PlanList({ plan }: { plan: PlanItem[] }) {
  if (plan.length === 0) return null;
  return (
    <div className="mb-2 rounded-lg border border-line bg-paper/70 px-3 py-2">
      <div className="mb-1 text-[9.5px] font-semibold tracking-wide text-teal">任务计划</div>
      {plan.map((item, index) => (
        <div key={`${item.title}-${index}`} className="flex items-center gap-2 text-[10.5px] leading-5">
          {item.status === "done" ? (
            <Check size={11} className="text-teal" />
          ) : item.status === "in_progress" ? (
            <LoaderCircle size={11} className="animate-spin text-amber" />
          ) : (
            <CircleDashed size={11} className="text-muted" />
          )}
          <span className={item.status === "done" ? "text-muted line-through" : "text-ink-soft"}>
            {item.title}
          </span>
        </div>
      ))}
    </div>
  );
}

/** 正在运行的一轮：交替显示模型叙述和工具调用。 */
export function LiveRun({ items, plan }: { items: LiveItem[]; plan: PlanItem[] }) {
  return (
    <div className="flex gap-3">
      <AgentAvatar busy />
      <div className="min-w-0 max-w-[86%] flex-1">
        <PlanList plan={plan} />
        <div className="rounded-xl rounded-tl-sm border border-line bg-paper-raised px-4 py-3">
          {items.length === 0 ? (
            <div className="flex items-center gap-2 text-[11px] text-muted">
              <LoaderCircle size={12} className="animate-spin" /> 正在读取项目…
            </div>
          ) : (
            items.map((item, index) =>
              item.type === "text" ? (
                <div key={index} className="my-1 text-[12px] leading-6 text-ink-soft">
                  <Markdown>{item.text}</Markdown>
                </div>
              ) : (
                <ToolRow key={item.trace.stepId} trace={item.trace} />
              ),
            )
          )}
        </div>
      </div>
    </div>
  );
}

export function AgentAvatar({ busy = false }: { busy?: boolean }) {
  return (
    <div
      className={`leaf-avatar mt-1 flex h-6 w-6 shrink-0 items-center justify-center rounded-full text-paper-raised ${
        busy ? "animate-pulse" : ""
      }`}
    >
      <svg width="12" height="12" viewBox="0 0 24 24" fill="none" aria-hidden>
        <path
          d="M5 19c0-8 5-13 14-14-1 9-6 14-14 14Z"
          fill="currentColor"
          opacity="0.92"
        />
        <path d="M5 19c3-4 6-7 10-10" stroke="var(--color-teal-deep)" strokeWidth="1.4" strokeLinecap="round" />
      </svg>
    </div>
  );
}

/** 历史消息上的执行轨迹：默认折叠，展开时按需读取。 */
export function RunTrace({
  runId,
  cached,
  onLoaded,
}: {
  runId: string;
  cached?: HarnessToolTrace[];
  onLoaded: (runId: string, traces: HarnessToolTrace[]) => void;
}) {
  const [open, setOpen] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    if (cached) return;
    void ipc
      .harnessTraces(runId)
      .then((traces) => onLoaded(runId, traces))
      .catch((cause) => setError(cause instanceof Error ? cause.message : String(cause)));
  }, [cached, onLoaded, runId]);
  if (error) {
    return (
      <div className="mb-1.5 flex items-center gap-1 text-[9.5px] text-coral">
        <AlertCircle size={10} /> {error}
      </div>
    );
  }
  if (!cached || cached.length === 0) return null;
  const proposals = cached.reduce((sum, trace) => sum + trace.candidateIds.length, 0);
  return (
    <div className="mb-1.5">
      <button
        onClick={() => setOpen((value) => !value)}
        className="flex items-center gap-1 rounded-md px-1 py-0.5 text-[9.5px] text-muted hover:bg-ink/5 hover:text-teal"
      >
        {open ? <ChevronDown size={10} /> : <ChevronRight size={10} />}
        执行轨迹 · {cached.length} 步{proposals ? ` · ${proposals} 项候选` : ""}
      </button>
      {open ? (
        <div className="mt-1 rounded-lg border border-line bg-paper/60 px-2.5 py-1.5">
          {cached.map((trace) => (
            <ToolRow key={trace.stepId} trace={trace} />
          ))}
        </div>
      ) : null}
    </div>
  );
}

export function ActionCards({
  actions,
  disabled,
  onAnswer,
  describe,
  onChanged,
  onNavigate,
}: {
  actions: AgentAction[];
  disabled: boolean;
  onAnswer: (text: string) => void;
  describe: CandidateDescriber;
  onChanged: () => void | Promise<void>;
  onNavigate: (target: "seed" | "studio") => void;
}) {
  if (actions.length === 0) return null;
  return (
    <div className="mt-2 space-y-2">
      {actions.map((action, index) =>
        action.type === "ask_author" ? (
          disabled ? null : (
            <div key={index} className="deco-card rounded-xl border border-amber/30 bg-amber-soft/35 px-3 py-2.5">
              <div className="mb-1.5 flex items-center gap-1.5 text-[10.5px] font-semibold text-amber">
                <HelpCircle size={11} /> 需要你决定
              </div>
              <div className="mb-2 text-[11.5px] leading-5 text-ink">{action.question}</div>
              <div className="flex flex-wrap gap-1.5">
                {action.options.map((option) => (
                  <button
                    key={option}
                    onClick={() => onAnswer(option)}
                    className="rounded-full border border-line bg-paper-raised px-2.5 py-1 text-left text-[10.5px] text-ink-soft transition hover:border-teal/50 hover:text-teal"
                  >
                    {option}
                  </button>
                ))}
              </div>
            </div>
          )
        ) : (
          <ApprovalCard
            key={index}
            action={action}
            describe={describe}
            onChanged={onChanged}
            onNavigate={onNavigate}
          />
        ),
      )}
    </div>
  );
}

type ItemState = { status: "done" | "rejected" | "error"; note: string };

function itemKey(item: ApprovalItem): string {
  return item.kind === "outline" ? `outline:${item.ids.join(",")}` : `${item.kind}:${item.id}`;
}

/** 聊天内确认卡：确认动作始终由作者点击完成，Agent 无权代为确认。 */
function ApprovalCard({
  action,
  describe,
  onChanged,
  onNavigate,
}: {
  action: Extract<AgentAction, { type: "approval" }>;
  describe: CandidateDescriber;
  onChanged: () => void | Promise<void>;
  onNavigate: (target: "seed" | "studio") => void;
}) {
  const [states, setStates] = useState<Record<string, ItemState>>({});
  const [busy, setBusy] = useState<string | null>(null);

  const mark = (key: string, state: ItemState) => setStates((current) => ({ ...current, [key]: state }));

  const run = async (item: ApprovalItem, accept: boolean) => {
    const key = itemKey(item);
    setBusy(key);
    try {
      if (item.kind === "premise") {
        if (accept) {
          await ipc.premiseConfirm(item.id);
          const next: Record<string, ItemState> = {};
          for (const other of action.items) {
            if (other.kind === "premise") {
              next[itemKey(other)] =
                other.id === item.id
                  ? { status: "done", note: "已确认为种子卡" }
                  : { status: "rejected", note: "已归档" };
            }
          }
          setStates((current) => ({ ...current, ...next }));
        } else {
          await ipc.premiseArchive(item.id);
          mark(key, { status: "rejected", note: "已归档" });
        }
      } else if (item.kind === "blueprint") {
        if (item.blocked) {
          onNavigate("seed");
          return;
        }
        await ipc.blueprintConfirm(item.id);
        mark(key, { status: "done", note: "蓝本已确认" });
      } else if (item.kind === "outline") {
        await ipc.outlineConfirm(item.ids);
        mark(key, { status: "done", note: "已确认并建立章节" });
      } else {
        const info = describe(item.id);
        if (info && ["draft", "draft_refined"].includes(info.kind)) {
          onNavigate("studio");
          return;
        }
        if (accept) await ipc.candidateAccept(item.id);
        else await ipc.candidateReject(item.id);
        mark(key, accept ? { status: "done", note: "已确认" } : { status: "rejected", note: "已拒绝" });
      }
      await onChanged();
    } catch (cause) {
      mark(key, { status: "error", note: cause instanceof Error ? cause.message : String(cause) });
    } finally {
      setBusy(null);
    }
  };

  const pendingCandidates = action.items.filter(
    (item) =>
      item.kind === "candidate" &&
      !states[itemKey(item)] &&
      describe(item.id) &&
      !["draft", "draft_refined"].includes(describe(item.id)?.kind ?? "") &&
      describe(item.id)?.risk !== "L3",
  );

  const acceptAll = async () => {
    for (const item of pendingCandidates) {
      // 逐条确认：每条仍走各自的校验与事务。
      // eslint-disable-next-line no-await-in-loop
      await run(item, true);
    }
  };

  // 已在别处处理过的候选折叠成一行，只列出仍需决定或本卡刚处理的条目。
  const visibleItems = action.items.filter(
    (item) => item.kind !== "candidate" || states[itemKey(item)] || describe(item.id),
  );
  const handledElsewhere = action.items.length - visibleItems.length;
  if (visibleItems.length === 0) {
    return (
      <div className="px-1 text-[10.5px] text-muted">
        {action.title}：{handledElsewhere} 项均已处理
      </div>
    );
  }

  return (
    <div className="deco-card rounded-xl border border-silk/70 bg-paper-raised/90 px-3 py-2.5">
      <div className="mb-1.5 flex items-center gap-1.5 text-[10.5px] font-semibold text-teal-deep">
        <Check size={11} /> {action.title}
        {action.group === "candidates" && pendingCandidates.length > 1 ? (
          <button
            onClick={() => void acceptAll()}
            disabled={Boolean(busy)}
            className="ml-auto rounded-full border border-teal/30 px-2 py-0.5 text-[10px] font-medium text-teal hover:bg-teal-soft disabled:opacity-50"
          >
            全部确认（{pendingCandidates.length}）
          </button>
        ) : null}
      </div>
      <ul className="space-y-1.5">
        {visibleItems.map((item) => {
          const key = itemKey(item);
          const state = states[key];
          const info = item.kind === "candidate" ? describe(item.id) : null;
          const handled = item.kind === "candidate" && !info && !state;
          const title =
            item.kind === "candidate" ? (info ? `${info.label} · ${info.summary}` : "候选已处理") : item.title;
          const summary = item.kind === "candidate" ? "" : item.summary;
          return (
            <li key={key} className="flex items-start gap-2 rounded-lg bg-paper/70 px-2.5 py-2">
              <div className="min-w-0 flex-1">
                <div className={`text-[11.5px] leading-5 ${handled ? "text-muted" : "text-ink"}`}>{title}</div>
                {summary ? <div className="text-[10.5px] leading-5 text-muted">{summary}</div> : null}
                {state ? (
                  <div
                    className={`text-[10.5px] leading-5 ${
                      state.status === "error" ? "text-coral" : state.status === "done" ? "text-teal" : "text-muted"
                    }`}
                  >
                    {state.note}
                  </div>
                ) : null}
              </div>
              {!state && !handled ? (
                <div className="flex shrink-0 items-center gap-1">
                  {busy === key ? <LoaderCircle size={12} className="animate-spin text-teal" /> : null}
                  {item.kind === "blueprint" && item.blocked ? (
                    <button
                      onClick={() => void run(item, true)}
                      className="rounded-full border border-amber/40 px-2 py-0.5 text-[10px] text-amber hover:bg-amber-soft"
                    >
                      先处理阻断
                    </button>
                  ) : item.kind === "candidate" && ["draft", "draft_refined"].includes(info?.kind ?? "") ? (
                    <button
                      onClick={() => void run(item, true)}
                      className="rounded-full border border-teal/30 px-2 py-0.5 text-[10px] text-teal hover:bg-teal-soft"
                    >
                      去正文页对照
                    </button>
                  ) : (
                    <>
                      <button
                        disabled={Boolean(busy)}
                        onClick={() => void run(item, true)}
                        className="rounded-full bg-teal px-2.5 py-0.5 text-[10px] text-paper-raised hover:bg-teal-deep disabled:opacity-50"
                      >
                        {item.kind === "premise" ? "选这张" : "确认"}
                      </button>
                      {item.kind !== "blueprint" && item.kind !== "outline" ? (
                        <button
                          disabled={Boolean(busy)}
                          onClick={() => void run(item, false)}
                          className="rounded-full px-2 py-0.5 text-[10px] text-muted hover:bg-ink/5 hover:text-coral disabled:opacity-50"
                        >
                          {item.kind === "premise" ? "不要" : "拒绝"}
                        </button>
                      ) : null}
                    </>
                  )}
                </div>
              ) : null}
            </li>
          );
        })}
      </ul>
      {handledElsewhere > 0 ? (
        <div className="mt-1.5 text-[10px] text-muted">另有 {handledElsewhere} 项已处理</div>
      ) : null}
      {action.group === "premise" || action.group === "blueprint" || action.group === "outline" ? (
        <button
          onClick={() => onNavigate("seed")}
          className="mt-1.5 text-[10px] text-muted hover:text-teal"
        >
          在“灵感与卷章”中查看完整内容 →
        </button>
      ) : null}
    </div>
  );
}

export function ModeSwitch({
  mode,
  onChange,
}: {
  mode: AgentMode;
  onChange: (mode: AgentMode) => void;
}) {
  return (
    <div className="flex rounded-full border border-line bg-paper/70 p-0.5" role="radiogroup" aria-label="Agent 模式">
      {(
        [
          ["plan", "规划", "只读：阅读、检索、诊断和提问，不提出候选"],
          ["write", "执笔", "可提出候选；所有写入仍需你确认"],
        ] as const
      ).map(([key, label, title]) => (
        <button
          key={key}
          role="radio"
          aria-checked={mode === key}
          title={title}
          onClick={() => onChange(key)}
          className={`rounded-full px-2.5 py-0.5 text-[9.5px] transition ${
            mode === key ? "bg-teal text-paper-raised" : "text-muted hover:text-ink"
          }`}
        >
          {label}
        </button>
      ))}
    </div>
  );
}

/** 输入框末尾的 @路径 补全。 */
export function mentionQuery(text: string): string | null {
  const match = /@(\/[^\s@]*)?$/.exec(text);
  if (!match) return null;
  return match[1] ?? "/";
}

export function MentionList({
  query,
  paths,
  onPick,
}: {
  query: string;
  paths: string[];
  onPick: (path: string) => void;
}) {
  const needle = query.toLowerCase();
  const matches = paths
    .filter((path) => path.toLowerCase().includes(needle.replace(/^\//, "")) || path.startsWith(query))
    .slice(0, 8);
  if (matches.length === 0) return null;
  return (
    <div className="absolute bottom-full left-0 z-20 mb-1 w-[320px] rounded-xl border border-line bg-paper-raised p-1 shadow-xl">
      <div className="px-2 py-1 text-[9px] tracking-wide text-muted">引用项目资料（Agent 会先读取）</div>
      {matches.map((path) => (
        <button
          key={path}
          onMouseDown={(event) => {
            event.preventDefault();
            onPick(path);
          }}
          className="block w-full truncate rounded-lg px-2 py-1 text-left text-[11px] text-ink-soft hover:bg-teal-soft/70 hover:text-teal"
        >
          {path}
        </button>
      ))}
    </div>
  );
}
