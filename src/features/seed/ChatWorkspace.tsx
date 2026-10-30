import { Markdown } from "@/components/Markdown";
import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import * as DropdownMenu from "@radix-ui/react-dropdown-menu";
import {
  Archive,
  ArrowRight,
  ArrowUp,
  Copy,
  CornerDownLeft,
  ListChecks,
  LoaderCircle,
  MessageSquarePlus,
  Pin,
  Sparkles,
  Square,
  Trash2,
} from "lucide-react";
import { Badge, Button, Textarea } from "@/components/ui";
import {
  ActionCards,
  AgentAvatar,
  LiveRun,
  MentionList,
  ModeSwitch,
  RunTrace,
  actionsFromTraces,
  mentionQuery,
  type AgentMode,
  type CandidateDescriber,
  type LiveItem,
  type PlanItem,
} from "@/features/agent/HarnessParts";
import {
  ipc,
  onChatDelta,
  onBlueprintProgress,
  onHarnessEvent,
  type HarnessToolTrace,
  type AgentNextAction,
  type BeatOption,
  type Candidate,
  type ChapterRecord,
  type ChatMessage,
  type ChatSession,
  type Premise,
} from "@/lib/ipc";
import { useAppStore } from "@/stores/ui";

const modes = [
  { key: "diverge", label: "发散" },
  { key: "structure", label: "结构" },
  { key: "plot", label: "推进剧情" },
  { key: "lookup", label: "查设定" },
];

const captureKinds = [
  { key: "character", label: "人物" },
  { key: "setting", label: "设定" },
  { key: "place", label: "地点" },
  { key: "rule", label: "规则" },
  { key: "promise", label: "伏笔" },
  { key: "scene_goal", label: "章节目标" },
  { key: "style_sample", label: "文笔样本" },
];

/** 只有显式的斜杠命令直接启动工作流；自然语言交给 Agent 理解并调用工具。 */
function detectWorkflowAction(content: string): string | null {
  const text = content.trim();
  if (/^\/灵感(?:\s|$)/.test(text)) return "seed_generate";
  if (/^\/蓝本(?:\s|$)/.test(text)) return "blueprint_generate";
  if (/^\/卷章(?:\s|$)/.test(text)) return "outline_generate";
  if (/^\/文笔(?:\s|$)/.test(text)) return "style_proposal";
  return null;
}

export function ChatWorkspace({
  premise,
  surface = "seed",
  nextActions = [],
  onWorkflowComplete,
}: {
  premise: Premise | null;
  surface?: "seed" | "agent";
  nextActions?: AgentNextAction[];
  onWorkflowComplete?: () => void | Promise<void>;
}) {
  const [sessions, setSessions] = useState<ChatSession[]>([]);
  const [active, setActive] = useState<ChatSession | null>(null);
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [candidates, setCandidates] = useState<Candidate[]>([]);
  const [chapters, setChapters] = useState<ChapterRecord[]>([]);
  const [chapterId, setChapterId] = useState<string>("");
  const [beats, setBeats] = useState<BeatOption[]>([]);
  const [beatInstruction, setBeatInstruction] = useState("");
  const [beatBusy, setBeatBusy] = useState(false);
  const [workflowBusy, setWorkflowBusy] = useState<string | null>(null);
  const [input, setInput] = useState("");
  const [streaming, setStreaming] = useState("");
  const [sending, setSending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [agentMode, setAgentMode] = useState<AgentMode>(() => {
    try {
      return window.localStorage.getItem("cw.agentMode") === "plan" ? "plan" : "write";
    } catch {
      return "write";
    }
  });
  const [live, setLive] = useState<LiveItem[] | null>(null);
  const [livePlan, setLivePlan] = useState<PlanItem[]>([]);
  const [tracesByRun, setTracesByRun] = useState<Record<string, HarnessToolTrace[]>>({});
  const [paths, setPaths] = useState<string[]>([]);
  const requestRef = useRef<string | null>(null);
  const scrollRef = useRef<HTMLDivElement>(null);
  const refreshProject = useAppStore((s) => s.refreshProject);
  const notify = useAppStore((s) => s.notify);
  const setNav = useAppStore((s) => s.setNav);
  const agentDraft = useAppStore((s) => s.agentDraft);
  const setAgentDraft = useAppStore((s) => s.setAgentDraft);

  useEffect(() => {
    if (surface !== "agent" || !agentDraft) return;
    setInput(agentDraft);
    setAgentDraft(null);
  }, [agentDraft, setAgentDraft, surface]);

  const loadSessions = useCallback(async (selectId?: string) => {
    const list = await ipc.chatSessions(null);
    setSessions(list);
    setActive((current) => {
      const next =
        list.find((session) => session.id === (selectId ?? current?.id)) ?? list[0] ?? null;
      return next;
    });
  }, []);

  const loadActive = useCallback(async (session: ChatSession) => {
    const [history, pending] = await Promise.all([
      ipc.chatMessages(session.id),
      ipc.chatCandidates(session.id),
    ]);
    setMessages(history);
    setCandidates(pending.filter((c) => c.status === "pending"));
  }, []);

  useEffect(() => {
    (async () => {
      try {
        const [list, chapterList] = await Promise.all([
          ipc.chatSessions(null),
          ipc.chapters().catch(() => []),
        ]);
        setChapters(chapterList);
        setChapterId(chapterList[0]?.id ?? "");
        let first = list[0];
        if (!first) {
          first = await ipc.chatCreate({
            scope: surface === "agent" ? "lookup" : "seed",
            mode: surface === "agent" ? "structure" : "diverge",
            title: surface === "agent" ? "协作任务" : "灵感孵化",
            sceneId: null,
            branchId: null,
            parentSessionId: null,
          });
        }
        setSessions(first ? [first, ...list.filter((s) => s.id !== first.id)] : list);
        setActive(first ?? null);
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
      } finally {
        setLoading(false);
      }
    })();
  }, [surface]);

  useEffect(() => {
    if (!active) return;
    void loadActive(active).catch((e) =>
      setError(e instanceof Error ? e.message : String(e)),
    );
  }, [active, loadActive]);

  useEffect(() => {
    let dispose = () => {};
    void onChatDelta((event) => {
      if (event.sessionId !== active?.id) return;
      requestRef.current = event.requestId;
      setStreaming((prev) => prev + event.delta);
    }).then((unlisten) => {
      dispose = unlisten;
    });
    return () => dispose();
  }, [active?.id]);

  useEffect(() => {
    if (surface !== "agent") return;
    let dispose = () => {};
    void onHarnessEvent((event) => {
      if (event.sessionId !== active?.id) return;
      if (event.kind === "delta") {
        setLive((items) => {
          const next = [...(items ?? [])];
          const last = next.at(-1);
          if (last?.type === "text") next[next.length - 1] = { type: "text", text: last.text + event.text };
          else next.push({ type: "text", text: event.text });
          return next;
        });
      } else if (event.kind === "tool_start") {
        const data = event.data ?? {};
        setLive((items) => [
          ...(items ?? []),
          {
            type: "tool",
            trace: {
              stepId: String(data.stepId),
              seq: Number(data.seq ?? 0),
              toolName: String(data.toolName ?? ""),
              title: event.text,
              input: data.input ?? {},
              summary: "",
              status: "running",
              candidateIds: [],
              action: null,
            },
          },
        ]);
      } else if (event.kind === "tool_end") {
        const trace = event.data as HarnessToolTrace;
        setLive((items) =>
          (items ?? []).map((item) =>
            item.type === "tool" && item.trace.stepId === trace.stepId ? { type: "tool", trace } : item,
          ),
        );
      } else if (event.kind === "plan") {
        setLivePlan(Array.isArray(event.data) ? (event.data as PlanItem[]) : []);
      }
    }).then((unlisten) => {
      dispose = unlisten;
    });
    return () => dispose();
  }, [active?.id, surface]);

  // 分层构建故事圣经耗时较长：把阶段进度挂到正在运行的工具行上。
  useEffect(() => {
    if (surface !== "agent") return;
    let dispose = () => {};
    void onBlueprintProgress((event) => {
      setLive((items) =>
        items
          ? items.map((item) =>
              item.type === "tool" && item.trace.toolName === "generate_blueprint" && item.trace.status === "running"
                ? { type: "tool", trace: { ...item.trace, summary: `${event.label}（${event.step}/${event.total}）` } }
                : item,
            )
          : items,
      );
    }).then((unlisten) => {
      dispose = unlisten;
    });
    return () => dispose();
  }, [surface]);

  useEffect(() => {
    if (surface !== "agent") return;
    void ipc
      .harnessPaths()
      .then(setPaths)
      .catch(() => setPaths([]));
  }, [surface, messages.length]);

  const rememberTraces = useCallback((runId: string, traces: HarnessToolTrace[]) => {
    setTracesByRun((current) => ({ ...current, [runId]: traces }));
  }, []);

  const describeCandidate = useCallback<CandidateDescriber>(
    (id) => {
      const candidate = candidates.find((item) => item.id === id);
      if (!candidate) return null;
      return {
        label: candidateLabel(candidate.kind),
        summary: summaryOf(candidate),
        kind: candidate.kind,
        risk: candidate.riskLevel,
      };
    },
    [candidates],
  );

  const refreshAfterApproval = useCallback(async () => {
    if (!active) return;
    const pending = await ipc.chatCandidates(active.id);
    setCandidates(pending.filter((candidate) => candidate.status === "pending"));
    await refreshProject();
    await onWorkflowComplete?.();
  }, [active, onWorkflowComplete, refreshProject]);

  const changeAgentMode = (mode: AgentMode) => {
    setAgentMode(mode);
    try {
      window.localStorage.setItem("cw.agentMode", mode);
    } catch {
      // 偏好只是界面便利，存不下也不影响功能。
    }
  };

  useEffect(() => {
    const node = scrollRef.current;
    if (node) node.scrollTop = node.scrollHeight;
  }, [messages, streaming, live]);

  const newSession = async () => {
    try {
      const scope =
        active?.mode === "plot"
          ? "plot"
          : active?.mode === "lookup"
            ? "lookup"
            : surface === "agent"
              ? "lookup"
              : "seed";
      const session = await ipc.chatCreate({
        scope,
        mode: active?.mode ?? "diverge",
        title: null,
        sceneId: null,
        branchId: null,
        parentSessionId: null,
      });
      await loadSessions(session.id);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const sendChat = async (preset?: string) => {
    const content = (preset ?? input).trim();
    if (!content || !active || sending) return;
    setInput("");
    setSending(true);
    setStreaming("");
    setError(null);
    requestRef.current = null;
    const temp: ChatMessage = {
      id: `temp-${Date.now()}`,
      sessionId: active.id,
      seq: 999999,
      role: "user",
      content,
      model: "",
      tokens: 0,
      createdAt: new Date().toISOString(),
      linkedCandidateIds: [],
      pinned: false,
      agentRunId: null,
    };
    setMessages((prev) => [...prev, temp]);
    try {
      const result = await ipc.chatSend(active.id, content);
      setMessages((prev) => [
        ...prev.filter((message) => message.id !== temp.id),
        result.userMessage,
        result.assistantMessage,
      ]);
      setActive(result.session);
      setStreaming("");
      await loadSessions(active.id);
      await refreshProject();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setMessages((prev) => prev.filter((message) => message.id !== temp.id));
      setStreaming("");
    } finally {
      setSending(false);
      requestRef.current = null;
    }
  };

  const sendHarness = async (content: string) => {
    if (!active || sending) return;
    setInput("");
    setSending(true);
    setError(null);
    setLive([]);
    setLivePlan([]);
    const temp: ChatMessage = {
      id: `temp-${Date.now()}`,
      sessionId: active.id,
      seq: 999999,
      role: "user",
      content,
      model: "",
      tokens: 0,
      createdAt: new Date().toISOString(),
      linkedCandidateIds: [],
      pinned: false,
      agentRunId: null,
    };
    setMessages((prev) => [...prev, temp]);
    try {
      const result = await ipc.harnessSend(active.id, content, agentMode);
      setTracesByRun((current) => ({ ...current, [result.runId]: result.traces }));
      setMessages((prev) => [
        ...prev.filter((message) => message.id !== temp.id),
        result.userMessage,
        result.assistantMessage,
      ]);
      setActive(result.session);
      const pending = await ipc.chatCandidates(active.id);
      setCandidates(pending.filter((candidate) => candidate.status === "pending"));
      await loadSessions(active.id);
      await refreshProject();
      await onWorkflowComplete?.();
      if (result.candidates.length > 0) {
        notify(`Agent 提出了 ${result.candidates.length} 项候选，确认前不会改变权威资料`);
      }
    } catch (e) {
      const message = e instanceof Error ? e.message : String(e);
      if (message !== "已取消") setError(message);
      try {
        await loadActive(active);
      } catch {
        // 保留原始错误。
      }
    } finally {
      setSending(false);
      setLive(null);
      setLivePlan([]);
    }
  };

  const finishWorkflow = async () => {
    if (!active) return;
    await Promise.all([
      loadActive(active),
      loadSessions(active.id),
      refreshProject(),
      onWorkflowComplete?.(),
    ]);
  };

  const runWorkflow = async (workflow: string, instruction: string) => {
    if (!active || workflowBusy || sending) return;
    const content =
      instruction.trim() ||
      (workflow === "seed_generate"
        ? premise?.logline ?? ""
        : "请根据当前项目状态执行这个工作流，并说明完成标准。");
    if (workflow === "chat") {
      await sendChat(content);
      return;
    }
    if (workflow === "review_candidates") {
      setNav("audit");
      return;
    }
    if (workflow === "open_seed") {
      setNav("seed");
      return;
    }
    if (workflow === "open_studio") {
      setNav("studio");
      return;
    }
    if (workflow === "seed_generate" && content.trim().length < 4) {
      setError("先在输入框写一个画面、冲突或一句话故事，再让 Agent 生成方向。");
      return;
    }
    setWorkflowBusy(workflow);
    setError(null);
    try {
      if (workflow === "seed_generate") {
        const result = await ipc.seedGenerate(content, active.id);
        notify(`已生成 ${result.directions.length} 个灵感方向，并保存在当前会话`);
      } else if (workflow === "blueprint_generate") {
        await ipc.chatAppendUser(active.id, content);
        const view = await ipc.blueprintGenerate({
          medium: "novel",
          structureStrategy: "adaptive",
          targetUnits: null,
        });
        const errors = view.diagnostics.filter((item) => item.severity === "error").length;
        const warnings = view.diagnostics.filter((item) => item.severity === "warning").length;
        await ipc.chatAppendAssistant(
          active.id,
          `已生成叙事蓝本候选《${view.blueprint.title}》，包含 ${view.elements.length} 条人物/势力/规则、${view.timeline.length} 个时间锚点、${view.nodes.length} 个逻辑节点。诊断：${errors} 阻断、${warnings} 提醒。\n\n下一步：先审阅叙事蓝本并处理阻断项，确认后才会用于卷章规划。`,
        );
        notify("叙事蓝本候选已生成；确认前不会进入正式规划");
      } else if (workflow === "outline_generate") {
        await ipc.chatAppendUser(active.id, content);
        const nodes = await ipc.outlineGenerate({
          mode: "adaptive",
          totalChapters: null,
          volumes: [],
        });
        await ipc.chatAppendAssistant(
          active.id,
          `已生成 ${nodes.length} 个卷章候选节点，并绑定已确认叙事蓝本的逻辑节点。\n\n下一步：逐项检查因果承接、节奏和角色行动，再确认进入正文工坊。`,
        );
        notify("卷章骨架候选已生成，请审阅后确认");
      } else if (workflow === "style_proposal") {
        const result = await ipc.agentStyleProposal(active.id, content);
        notify(`已生成文笔方案候选：${result.presetName}`);
      } else {
        throw new Error(`未知 Agent 工作流：${workflow}`);
      }
      setInput("");
      await finishWorkflow();
      const pending = await ipc.chatCandidates(active.id);
      setCandidates(pending.filter((candidate) => candidate.status === "pending"));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      try {
        await finishWorkflow();
      } catch {
        // Preserve the workflow error.
      }
    } finally {
      setWorkflowBusy(null);
    }
  };

  // “下一步”建议：导航类直接跳转；其它都作为一句话交给 Agent，由它先问清再调用工具。
  const runNextAction = async (action: AgentNextAction) => {
    if (["review_candidates", "open_seed", "open_studio"].includes(action.action)) {
      await runWorkflow(action.action, action.prompt);
      return;
    }
    if (surface === "agent") {
      await sendHarness(action.prompt);
      return;
    }
    await runWorkflow(action.action, action.prompt);
  };

  const send = async (preset?: string) => {
    const content = (preset ?? input).trim();
    const detected = surface === "agent" ? detectWorkflowAction(content) : null;
    if (detected) {
      await runWorkflow(detected, content.replace(/^\/(灵感|蓝本|卷章|文笔)\s*/, ""));
      return;
    }
    if (surface === "agent") {
      if (content) await sendHarness(content);
      return;
    }
    await sendChat(preset);
  };

  const cancel = async () => {
    if (surface === "agent" && active) {
      await ipc.harnessCancel(active.id);
      return;
    }
    if (requestRef.current) await ipc.chatCancel(requestRef.current);
  };

  const updateMode = async (mode: string) => {
    if (!active) return;
    try {
      const session = await ipc.chatUpdate(active.id, {
        mode,
        scope: mode === "plot" ? "plot" : mode === "lookup" ? "lookup" : active.scope,
      });
      setActive(session);
      await loadSessions(session.id);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const capture = async (message: ChatMessage, kind: string) => {
    if (!active) return;
    try {
      await ipc.chatCapture({
        sessionId: active.id,
        messageId: message.id,
        kind,
        text: message.content,
      });
      const pending = await ipc.chatCandidates(active.id);
      setCandidates(pending.filter((c) => c.status === "pending"));
      await refreshProject();
      notify("已加入候选区");
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const acceptCandidate = async (id: string) => {
    if (!active) return;
    try {
      const target = candidates.find((candidate) => candidate.id === id);
      if (target && ["draft", "draft_refined"].includes(target.kind)) {
        setNav("studio");
        notify("正文草稿请在正文页对照原文后采用");
        return;
      }
      await ipc.candidateAccept(id);
      const pending = await ipc.chatCandidates(active.id);
      setCandidates(pending.filter((c) => c.status === "pending"));
      await refreshProject();
      await onWorkflowComplete?.();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const togglePin = async () => {
    if (!active) return;
    const session = await ipc.chatUpdate(active.id, { pinned: !active.pinned });
    setActive(session);
    await loadSessions(session.id);
  };

  const fork = async () => {
    if (!active) return;
    const session = await ipc.chatFork(active.id);
    await loadSessions(session.id);
  };

  const archiveSession = async () => {
    if (!active) return;
    await ipc.chatArchive(active.id);
    await loadSessions();
  };

  const removeSession = async () => {
    if (!active) return;
    if (!window.confirm("删除这个会话？消息会被删除，但已确认事实与候选不会受影响。")) return;
    await ipc.chatDelete(active.id);
    await loadSessions();
  };

  const generateBeats = async () => {
    if (!active || !beatInstruction.trim()) return;
    setBeatBusy(true);
    setError(null);
    try {
      const result = await ipc.plotBeats(active.id, beatInstruction.trim());
      setBeats(result);
      await loadActive(active);
      await refreshProject();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBeatBusy(false);
    }
  };

  const continueWrite = async () => {
    if (!active) return;
    setBeatBusy(true);
    try {
      await ipc.chatContinue(active.id);
      setCandidates(await ipc.chatCandidates(active.id));
      await refreshProject();
      notify("续写草稿已进入候选区（500–800 字）");
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBeatBusy(false);
    }
  };

  const toScene = async () => {
    if (!active || !chapterId) {
      setError("请先在骨架中确认章节，再写成场景");
      return;
    }
    setBeatBusy(true);
    try {
      await ipc.chatToScene(active.id, chapterId);
      notify("场景契约已生成，请到正文工坊查看");
      await refreshProject();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBeatBusy(false);
    }
  };

  if (loading) {
    return (
      <div className="flex h-64 items-center justify-center text-[12px] text-muted">
        <LoaderCircle size={15} className="mr-2 animate-spin" /> 正在打开创作会话
      </div>
    );
  }

  return (
    <div
      className={`grid gap-3 ${
        surface === "agent"
          ? "workspace-fill min-h-[480px] grid-cols-[240px_minmax(0,1fr)_320px]"
          : "grid-cols-[210px_minmax(0,1fr)_280px]"
      }`}
      style={surface === "agent" ? undefined : { height: "calc(100vh - 245px)", minHeight: 520 }}
    >
      <aside className="paper-card flex min-h-0 flex-col overflow-hidden">
        <div className="flex items-center justify-between border-b border-line px-3 py-2.5">
          <span className="text-[11px] font-semibold tracking-wide text-ink-soft">
            {surface === "agent" ? "任务会话" : "会话"}
          </span>
          <button
            onClick={() => void newSession()}
            className="focus-ring flex h-6 w-6 items-center justify-center rounded-md text-muted hover:bg-ink/5 hover:text-teal"
            title="新建会话"
          >
            <MessageSquarePlus size={14} />
          </button>
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto p-1.5">
          {sessions.map((session) => (
            <button
              key={session.id}
              onClick={() => setActive(session)}
              className={`group mb-1 w-full rounded-lg px-2.5 py-2 text-left transition ${
                active?.id === session.id ? "bg-teal-soft/80" : "hover:bg-ink/[0.035]"
              }`}
            >
              <div className="flex items-center gap-1.5">
                {session.pinned ? <Pin size={9} className="shrink-0 text-amber" /> : null}
                <span className="truncate text-[11.5px] font-medium text-ink">{session.title}</span>
              </div>
              <div className="mt-1 flex items-center gap-1.5 text-[9.5px] text-muted">
                <span>{scopeLabel(session.scope)}</span>
                <span className="h-0.5 w-0.5 rounded-full bg-muted/50" />
                <span>{modeLabel(session.mode)}</span>
                {session.pendingCandidates > 0 ? (
                  <span className="ml-auto rounded-full bg-amber-soft px-1.5 text-amber">
                    {session.pendingCandidates}
                  </span>
                ) : null}
              </div>
            </button>
          ))}
        </div>
        <div className="flex items-center justify-around border-t border-line px-1 py-1.5">
          <IconAction label={active?.pinned ? "取消置顶" : "置顶"} onClick={() => void togglePin()}>
            <Pin size={12} />
          </IconAction>
          <IconAction label="分叉会话" onClick={() => void fork()}>
            <Copy size={12} />
          </IconAction>
          <IconAction label="归档会话" onClick={() => void archiveSession()}>
            <Archive size={12} />
          </IconAction>
          <IconAction label="删除会话" onClick={() => void removeSession()}>
            <Trash2 size={12} />
          </IconAction>
        </div>
      </aside>

      <section className="paper-card flex min-h-0 flex-col overflow-hidden">
        <div className="flex items-center justify-between border-b border-line px-4 py-2.5">
          <div className="flex items-center gap-3">
            <span className="max-w-[260px] truncate text-[12px] font-semibold text-ink">
              {active?.title ?? (surface === "agent" ? "Agent 对话" : "创作对话")}
            </span>
            {surface === "agent" ? (
              <ModeSwitch mode={agentMode} onChange={changeAgentMode} />
            ) : (
            <div className="flex rounded-full border border-line p-0.5">
              {modes.map((mode) => (
                <button
                  key={mode.key}
                  onClick={() => void updateMode(mode.key)}
                  className={`rounded-full px-2 py-0.5 text-[9.5px] transition ${
                    active?.mode === mode.key
                      ? "bg-teal text-paper-raised"
                      : "text-muted hover:text-ink"
                  }`}
                >
                  {mode.label}
                </button>
              ))}
            </div>
            )}
          </div>
          {premise ? (
            <Badge tone={premise.status === "confirmed" ? "teal" : "amber"}>
              {premise.status === "confirmed" ? "种子卡已确认" : "种子卡候选"}
            </Badge>
          ) : (
            <Badge tone="amber">无种子卡</Badge>
          )}
        </div>

        {surface === "agent" && nextActions.length > 0 ? (
          <div className="flex items-start gap-3 border-b border-line bg-paper/55 px-4 py-2.5">
            <div className="mt-0.5 flex h-6 w-6 shrink-0 items-center justify-center rounded-full bg-teal-soft text-teal">
              <ListChecks size={12} />
            </div>
            <div className="min-w-0 flex-1">
              <div className="flex items-center gap-2 text-[10px] font-semibold text-teal">
                下一步
                <span className="font-normal text-muted">{nextActions[0].description}</span>
              </div>
              <div className="mt-1.5 flex flex-wrap gap-1.5">
                {nextActions.slice(0, 3).map((action, index) => (
                  <button
                    key={action.key}
                    onClick={() => void runNextAction(action)}
                    disabled={Boolean(workflowBusy) || sending}
                    className={`flex items-center gap-1 rounded-full px-2.5 py-1 text-[9.5px] transition disabled:opacity-50 ${
                      index === 0
                        ? "bg-teal text-paper-raised hover:bg-teal/90"
                        : "border border-line bg-paper-raised text-ink-soft hover:border-teal/40 hover:text-teal"
                    }`}
                  >
                    {workflowBusy === action.action ? (
                      <LoaderCircle size={10} className="animate-spin" />
                    ) : (
                      <ArrowRight size={10} />
                    )}
                    {action.label}
                  </button>
                ))}
              </div>
            </div>
          </div>
        ) : null}

        <div ref={scrollRef} className="min-h-0 flex-1 overflow-y-auto px-5 py-5">
          {messages.length === 0 && !streaming ? (
            <div className="flex h-full flex-col items-center justify-center text-center">
              <div className="mb-4 flex h-10 w-10 items-center justify-center rounded-full border border-teal/20 bg-teal-soft/40 text-teal">
                <CornerDownLeft size={16} />
              </div>
              <div className="display text-[18px] text-ink">
                {surface === "agent" ? "把任务交给 Agent，它会先读资料再提出候选" : "对话只产出建议，不直接成为事实"}
              </div>
              <p className="mt-2 max-w-md text-[11.5px] leading-6 text-muted">
                {surface === "agent"
                  ? "Agent 可以浏览人物、章节、伏笔与线程，检索原文，诊断章节工艺，并把修改整理成候选。用 @ 引用资料，确认权始终在你手里。"
                  : "讨论人物、规则或章节目标。AI 消息可以“采纳为”候选，确认后才进入正式表。"}
              </p>
              <div className="mt-5 flex flex-wrap justify-center gap-2">
                {(surface === "agent"
                  ? [
                      "看看项目现在处在哪个阶段，最该补的三件事是什么？",
                      "检查所有章节的价值转变和章末钩子，提出修订候选。",
                      "扫描已有正文，把人物、地点和伏笔整理成带原文证据的候选。",
                    ]
                  : [
                      "这个主角最大的秘密会怎样反噬他？",
                      "给我三个第一场可以发生的冲突。",
                      "这个世界规则有什么代价？",
                    ]
                ).map((prompt) => (
                  <button
                    key={prompt}
                    onClick={() => void send(prompt)}
                    className="rounded-full border border-line bg-paper-raised px-3 py-1.5 text-[10.5px] text-ink-soft transition hover:border-teal/40 hover:text-teal"
                  >
                    {prompt}
                  </button>
                ))}
              </div>
            </div>
          ) : (
            <div className="space-y-5">
              {messages.map((message, index) => (
                <MessageBubble
                  key={message.id}
                  message={message}
                  onCapture={(kind) => void capture(message, kind)}
                  traces={message.agentRunId ? tracesByRun[message.agentRunId] : undefined}
                  onTraces={rememberTraces}
                  actionsEnabled={index === messages.length - 1 && !sending && !workflowBusy}
                  onAnswer={(text) => void send(text)}
                  describe={describeCandidate}
                  onChanged={refreshAfterApproval}
                  onNavigate={(target) => setNav(target)}
                />
              ))}
              {live ? <LiveRun items={live} plan={livePlan} /> : null}
              {streaming ? (
                <div className="flex gap-3">
                  <div className="mt-1 flex h-6 w-6 shrink-0 items-center justify-center rounded-full bg-teal text-[9px] font-semibold text-paper-raised">
                    AI
                  </div>
                  <div className="max-w-[86%] rounded-xl rounded-tl-sm border border-line bg-paper-raised px-4 py-3 text-[12px] leading-6 text-ink-soft">
                    <Markdown>{streaming}</Markdown>
                    <span className="ml-1 inline-block h-3 w-0.5 animate-pulse bg-teal align-middle" />
                  </div>
                </div>
              ) : null}
            </div>
          )}
        </div>

        <div className="border-t border-line px-4 py-3">
          {surface === "agent" ? (
            <div className="mb-2.5 flex flex-wrap items-center gap-1.5">
              {[
                ["seed_generate", "灵感方向", "生成或继续完善三套灵感方向"],
                ["blueprint_generate", "叙事蓝本", "建立人物、规则、时间线与因果链候选"],
                ["outline_generate", "卷章骨架", "把已确认蓝本规划为卷章候选"],
                ["style_proposal", "文笔方案", "根据故事气质生成文笔档案候选"],
              ].map(([workflow, label, description]) => (
                <button
                  key={workflow}
                  title={description}
                  onClick={() => void runWorkflow(workflow, input)}
                  disabled={Boolean(workflowBusy) || sending}
                  className="flex items-center gap-1 rounded-full border border-line bg-paper-raised px-2.5 py-1 text-[9.5px] text-ink-soft transition hover:border-teal/40 hover:text-teal disabled:opacity-50"
                >
                  {workflowBusy === workflow ? (
                    <LoaderCircle size={10} className="animate-spin" />
                  ) : (
                    <Sparkles size={10} />
                  )}
                  {label}
                </button>
              ))}

            </div>
          ) : null}
          {active?.mode === "plot" || beats.length > 0 ? (
            <div className="mb-2.5 rounded-lg border border-amber/20 bg-amber-soft/30 px-3 py-2.5">
              <div className="mb-2 flex items-center gap-2">
                <span className="text-[10px] font-semibold text-amber">推进剧情 · 先给节拍，不写整章</span>
                <input
                  value={beatInstruction}
                  onChange={(e) => setBeatInstruction(e.target.value)}
                  placeholder="下一场你想发生什么？"
                  className="h-7 flex-1 rounded-md border border-line bg-paper-raised px-2 text-[10.5px] outline-none"
                  onKeyDown={(e) => {
                    if (e.key === "Enter") void generateBeats();
                  }}
                />
                <Button size="sm" tone="ghost" onClick={() => void generateBeats()} disabled={beatBusy}>
                  {beatBusy ? <LoaderCircle size={11} className="animate-spin" /> : null}
                  三个节拍
                </Button>
              </div>
              {beats.length > 0 ? (
                <div className="grid grid-cols-3 gap-2">
                  {beats.map((beat, index) => (
                    <div key={beat.title} className="rounded-md border border-line bg-paper-raised p-2">
                      <div className="text-[9.5px] font-semibold text-ink">
                        {String.fromCharCode(65 + index)} · {beat.title}
                      </div>
                      <div className="mt-1 line-clamp-3 text-[9px] leading-4 text-muted">{beat.beat}</div>
                      <button
                        onClick={() =>
                          void ipc
                            .sceneFromBeat(chapterId, beat)
                            .then(() => notify("场景契约已建立，可在正文工坊查看"))
                            .catch((e: unknown) =>
                              setError(e instanceof Error ? e.message : String(e)),
                            )
                        }
                        disabled={!chapterId}
                        className="mt-1.5 text-[9px] text-teal hover:underline disabled:text-muted"
                      >
                        写成场景 →
                      </button>
                    </div>
                  ))}
                </div>
              ) : null}
            </div>
          ) : null}
          {error ? <div className="mb-2 text-[11px] leading-5 text-coral">{error}</div> : null}
          <div className="relative">
            {surface === "agent" && mentionQuery(input) !== null ? (
              <MentionList
                query={mentionQuery(input) ?? "/"}
                paths={paths}
                onPick={(path) => setInput((value) => value.replace(/@(\/[^\s@]*)?$/, `@${path} `))}
              />
            ) : null}
            <Textarea
              rows={surface === "agent" ? 2 : 3}
              value={input}
              onChange={(e) => setInput(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && !e.shiftKey) {
                  e.preventDefault();
                  void send();
                }
              }}
              placeholder={
                surface === "agent"
                  ? "交给 Agent 的任务…  @ 引用资料，/蓝本 /卷章 启动工作流（Enter 发送）"
                  : "继续讨论，或让 AI 推演下一场…（Enter 发送，Shift+Enter 换行）"
              }
              disabled={sending}
              className="pr-24"
            />
            <div className="absolute bottom-2.5 right-2.5 flex items-center gap-1.5">
              {sending ? (
                <Button size="sm" tone="ghost" onClick={() => void cancel()} icon={<Square size={11} />}>
                  停止
                </Button>
              ) : null}
              <Button
                size="sm"
                tone="primary"
                onClick={() => void send()}
                disabled={!input.trim() || sending || Boolean(workflowBusy) || !active}
                icon={<ArrowUp size={13} />}
              >
                发送
              </Button>
            </div>
          </div>
          <div className="mt-2 flex items-center gap-3 text-[9.5px] text-muted">
            <span>
              {surface === "agent"
                ? agentMode === "plan"
                  ? "规划：只读分析"
                  : "执笔：写入只进候选区"
                : "对话不会自动写入正文或权威库"}
            </span>
            {surface === "agent" ? null : (
              <>
                <span className="h-0.5 w-0.5 rounded-full bg-muted/50" />
                <span>超过 12 轮建议先写第一场或生成骨架</span>
              </>
            )}
            <span className="ml-auto flex items-center gap-1.5">
              <select
                value={chapterId}
                onChange={(e) => setChapterId(e.target.value)}
                className="rounded border border-line bg-paper-raised px-1.5 py-0.5 text-[9px]"
              >
                <option value="">选择章节</option>
                {chapters.map((chapter) => (
                  <option key={chapter.id} value={chapter.id}>
                    第 {chapter.number} 章 {chapter.title}
                  </option>
                ))}
              </select>
              <button
                onClick={() => void continueWrite()}
                disabled={beatBusy}
                className="rounded border border-line px-1.5 py-0.5 hover:border-teal/40 hover:text-teal disabled:opacity-50"
              >
                继续写 500–800 字
              </button>
              <button
                onClick={() => void toScene()}
                disabled={beatBusy || !chapterId}
                className="rounded border border-line px-1.5 py-0.5 hover:border-teal/40 hover:text-teal disabled:opacity-50"
              >
                写成场景
              </button>
            </span>
          </div>
        </div>
      </section>

      <aside className="paper-card flex min-h-0 flex-col overflow-hidden">
        <div className="flex items-center justify-between border-b border-line px-3.5 py-2.5">
          <div>
            <div className="text-[11px] font-semibold text-ink-soft">候选区</div>
            <div className="text-[9.5px] text-muted">采纳后仍需确认</div>
          </div>
          <span className="flex h-5 min-w-5 items-center justify-center rounded-full bg-amber-soft px-1.5 text-[9.5px] font-semibold text-amber">
            {candidates.length}
          </span>
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto p-2.5">
          {candidates.length === 0 ? (
            <div className="px-2 py-8 text-center text-[10.5px] leading-5 text-muted">
              工作流生成的内容和 AI 消息“采纳为”的候选都会出现在这里。
            </div>
          ) : (
            <div className="space-y-2">
              {candidates.map((candidate) => (
                <div
                  key={candidate.id}
                  className="rounded-xl border border-dashed border-line-strong bg-paper/60 p-3"
                >
                  <div className="mb-1.5 flex items-center justify-between">
                    <span className="text-[10px] font-semibold text-teal">
                      {candidateLabel(candidate.kind)}
                    </span>
                    <span className="rounded bg-ink/5 px-1.5 py-0.5 text-[9px] text-muted">
                      {candidate.riskLevel}
                    </span>
                  </div>
                  <div className="text-[10.5px] leading-5 text-ink-soft">
                    {summaryOf(candidate)}
                  </div>
                  <div className="mt-2 flex justify-end">
                    <Button
                      size="sm"
                      tone="ghost"
                      onClick={() => void acceptCandidate(candidate.id)}
                    >
                      确认
                    </Button>
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
        <div className="border-t border-line px-3 py-2 text-[9.5px] leading-4 text-muted">
          未确认内容以虚线显示，不进入正文事实。
        </div>
      </aside>
    </div>
  );
}

function MessageBubble({
  message,
  onCapture,
  traces,
  onTraces,
  actionsEnabled,
  onAnswer,
  describe,
  onChanged,
  onNavigate,
}: {
  message: ChatMessage;
  onCapture: (kind: string) => void;
  traces?: HarnessToolTrace[];
  onTraces: (runId: string, traces: HarnessToolTrace[]) => void;
  actionsEnabled: boolean;
  onAnswer: (text: string) => void;
  describe: CandidateDescriber;
  onChanged: () => void | Promise<void>;
  onNavigate: (target: "seed" | "studio") => void;
}) {
  if (message.role === "user") {
    return (
      <div className="flex justify-end">
        <div className="max-w-[80%] rounded-xl rounded-tr-sm bg-teal px-4 py-2.5 text-[12px] leading-6 whitespace-pre-wrap text-paper-raised">
          {message.content}
        </div>
      </div>
    );
  }
  if (message.role === "system") return null;
  return (
    <div className="group flex gap-3">
      <AgentAvatar />
      <div className="max-w-[86%]">
        {message.agentRunId ? (
          <RunTrace runId={message.agentRunId} cached={traces} onLoaded={onTraces} />
        ) : null}
        <div className="rounded-xl rounded-tl-sm border border-line bg-paper-raised px-4 py-3 text-[12px] leading-6 text-ink-soft">
          <Markdown>{message.content}</Markdown>
        </div>
        {traces ? (
          <ActionCards
            actions={actionsFromTraces(traces)}
            disabled={!actionsEnabled}
            onAnswer={onAnswer}
            describe={describe}
            onChanged={onChanged}
            onNavigate={onNavigate}
          />
        ) : null}
        <div className="mt-1.5 flex items-center gap-2 opacity-0 transition group-hover:opacity-100">
          <DropdownMenu.Root>
            <DropdownMenu.Trigger asChild>
              <button className="rounded-md px-1.5 py-0.5 text-[9.5px] text-muted hover:bg-ink/5 hover:text-teal">
                采纳为…
              </button>
            </DropdownMenu.Trigger>
            <DropdownMenu.Portal>
              <DropdownMenu.Content
                align="start"
                sideOffset={4}
                className="z-50 min-w-[150px] rounded-xl border border-line bg-paper-raised p-1 shadow-xl"
              >
                {captureKinds.map((kind) => (
                  <DropdownMenu.Item
                    key={kind.key}
                    onSelect={() => onCapture(kind.key)}
                    className="cursor-pointer rounded-lg px-2.5 py-1.5 text-[11px] text-ink-soft outline-none hover:bg-teal-soft/70 hover:text-teal"
                  >
                    {kind.label}
                  </DropdownMenu.Item>
                ))}
              </DropdownMenu.Content>
            </DropdownMenu.Portal>
          </DropdownMenu.Root>
          {message.pinned ? <Badge tone="amber">会话约束</Badge> : null}
          <span className="text-[9px] text-muted/70">{message.model || "AI"}</span>
        </div>
      </div>
    </div>
  );
}

function IconAction({
  label,
  onClick,
  children,
}: {
  label: string;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <button
      title={label}
      onClick={onClick}
      className="focus-ring flex h-6 w-7 items-center justify-center rounded-md text-muted transition hover:bg-ink/5 hover:text-ink"
    >
      {children}
    </button>
  );
}

function scopeLabel(scope: string) {
  return { seed: "孵化", plot: "剧情", scene: "场景", lookup: "设定", audit: "审计" }[scope] ?? scope;
}

function modeLabel(mode: string) {
  return { diverge: "发散", structure: "结构", plot: "推进", direct: "续写", lookup: "查设定" }[mode] ?? mode;
}

function candidateLabel(kind: string) {
  return (
    {
      entity: "人物 / 设定",
      claim: "事实",
      event: "事件",
      relation: "关系",
      promise: "伏笔",
      rule: "规则",
      style_sample: "文笔样本",
      style_profile: "文笔档案",
      scene_goal: "章节目标",
      knowledge_entry: "知识条目",
      outline_patch: "大纲节点",
      craft_patch: "章节工艺卡",
      story_thread: "叙事线程",
      project_guide: "作者守则",
      draft: "正文草稿",
    }[kind] ?? kind
  );
}

export function summaryOf(candidate: Candidate): string {
  const p = candidate.payload ?? {};
  if (candidate.kind === "entity") {
    return `${p.canonicalName ?? p.name ?? "未命名"}${p.description ? `：${String(p.description).slice(0, 60)}` : ""}`;
  }
  if (candidate.kind === "claim") {
    return `${p.subject ?? ""} · ${p.predicate ?? ""} · ${p.object ?? ""}`;
  }
  if (candidate.kind === "event") {
    return String(p.title ?? p.description ?? "事件");
  }
  if (candidate.kind === "relation") {
    return `${p.from ?? ""} → ${p.to ?? ""}（${p.type ?? ""}）`;
  }
  if (candidate.kind === "promise") {
    return `${p.title ?? "伏笔"}：${String(p.contract ?? p.payoffContract ?? "").slice(0, 60)}`;
  }
  if (candidate.kind === "knowledge_entry") {
    return `${p.name ?? "未命名知识"}：${String(p.summary ?? "").slice(0, 60)}`;
  }
  if (candidate.kind === "outline_patch") {
    return `${p.level === "volume" ? "卷" : "章节"} · ${p.title ?? "未命名"}：${String(
      p.goal ?? "",
    ).slice(0, 60)}`;
  }
  if (candidate.kind === "craft_patch") {
    const changes = Object.entries(p.changes ?? {})
      .map(([key, value]) => `${craftFieldLabel(key)}：${String(value)}`)
      .join("；");
    return `第 ${p.chapter ?? "?"} 章 · ${changes}`.slice(0, 120);
  }
  if (candidate.kind === "story_thread") {
    return `${p.title ?? "线程"}：${String(p.question ?? "").slice(0, 60)}`;
  }
  if (candidate.kind === "project_guide") {
    return `守则修订：${String(p.content ?? "").slice(0, 80)}`;
  }
  if (candidate.kind === "draft") {
    return `第 ${p.chapterNumber ?? "?"} 章草稿 · ${p.wordCount ?? 0} 字：${String(p.excerpt ?? p.text ?? "").slice(0, 60)}`;
  }
  if (candidate.kind === "style_profile") {
    return `${p.name ?? "文笔档案"}：${String(p.agentReason ?? "等待作者确认具体文笔参数。").slice(0, 80)}`;
  }
  return String(p.text ?? p.description ?? p.title ?? "候选内容").slice(0, 90);
}

function craftFieldLabel(key: string): string {
  return (
    {
      valueShift: "价值转变",
      crisis: "危机",
      hook: "钩子",
      reveal: "揭示",
      goal: "目标",
      conflict: "冲突",
      turn: "转折",
      payoff: "兑现",
    }[key] ?? key
  );
}
