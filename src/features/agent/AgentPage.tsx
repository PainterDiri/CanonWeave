import { useCallback, useEffect, useMemo, useState } from "react";
import {
  AlertTriangle,
  Check,
  ChevronRight,
  CirclePause,
  CirclePlay,
  Clock3,
  FileText,
  ListChecks,
  LoaderCircle,
  MessageSquare,
  Plus,
  RotateCcw,
  Search,
  ShieldCheck,
  Sparkles,
  Square,
  Workflow,
  X,
} from "lucide-react";
import { Badge, Button, Modal, SectionTitle, Textarea } from "@/components/ui";
import { ChatWorkspace } from "@/features/seed/ChatWorkspace";
import {
  ipc,
  onAgentProgress,
  type AgentRun,
  type AgentRunCreateInput,
  type AgentRunDetail,
  type AgentNextAction,
  type AgentStep,
  type AgentToolDefinition,
  type Candidate,
  type FoundationCheck,
  type FoundationReport,
  type Premise,
} from "@/lib/ipc";
import { useAppStore } from "@/stores/ui";

type Surface = "workspace" | "chat";

const templates = [
  {
    label: "整合全书资料",
    goal: "分析当前项目的正文、导入稿和已确认资料，把可核实内容整理为人物、关系、势力、能力、地点、物品、时间线、伏笔和知识条目候选。重复名称先归并，所有候选附上原文证据。",
    output: "all",
    mode: "propose",
  },
  {
    label: "检查叙事地基",
    goal: "分析当前项目是否具备主角与对手、关系张力、势力、能力代价、世界约束、时间锚点和核心戏剧问题，并列出最需要补的内容。",
    output: "all",
    mode: "analyze",
  },
  {
    label: "梳理时间与伏笔",
    goal: "扫描现有正文与资料，区分故事时间、叙述时间和角色认知，整理时间线事件与尚未兑现的伏笔候选。",
    output: "timeline",
    mode: "propose",
  },
  {
    label: "同步知识库",
    goal: "把正文中可核实的势力、能力、地点、物品和世界规则整理为知识条目候选，标明代价、边界、来源和与已有条目的潜在重复。",
    output: "knowledge",
    mode: "propose",
  },
];

const scopeOptions = [
  { value: "project", label: "全书" },
  { value: "volume", label: "当前卷" },
  { value: "chapter", label: "当前章" },
  { value: "library", label: "指定资料库" },
];

const outputOptions = [
  { value: "all", label: "综合整理" },
  { value: "characters", label: "人物与关系" },
  { value: "timeline", label: "时间线" },
  { value: "promises", label: "伏笔" },
  { value: "knowledge", label: "知识库" },
  { value: "outline", label: "骨架与知识" },
];

const statusMeta: Record<string, { label: string; tone: "neutral" | "teal" | "amber" | "coral" }> = {
  draft: { label: "草稿", tone: "neutral" },
  planning: { label: "规划中", tone: "teal" },
  running: { label: "执行中", tone: "teal" },
  waiting_approval: { label: "待审批", tone: "amber" },
  paused: { label: "已暂停", tone: "amber" },
  canceled: { label: "已取消", tone: "neutral" },
  failed: { label: "失败", tone: "coral" },
  completed: { label: "已完成", tone: "teal" },
};

const stepMeta: Record<string, { label: string; icon: typeof FileText }> = {
  project_inventory: { label: "项目清单", icon: FileText },
  search_fts: { label: "原文检索", icon: Search },
  model_synthesis: { label: "证据归纳", icon: Sparkles },
  candidate_impact: { label: "影响检查", icon: AlertTriangle },
  foundation_check: { label: "地基检查", icon: ShieldCheck },
};

const kindLabels: Record<string, string> = {
  entity: "人物与实体",
  relation: "关系",
  event: "时间事件",
  promise: "伏笔",
  knowledge_entry: "知识条目",
  outline_patch: "骨架补丁",
  claim: "事实",
  rule: "规则",
  scene_goal: "章节目标",
  style_profile: "文笔档案",
  craft_patch: "章节工艺卡",
  story_thread: "叙事线程",
  project_guide: "作者守则",
  draft: "正文草稿",
};

function statusBadge(status: string) {
  const meta = statusMeta[status] ?? { label: status, tone: "neutral" as const };
  return <Badge tone={meta.tone}>{meta.label}</Badge>;
}

function payloadTitle(candidate: Candidate): string {
  const payload = candidate.payload ?? {};
  return (
    payload.canonicalName ??
    payload.title ??
    payload.name ??
    payload.type ??
    kindLabels[candidate.kind] ??
    candidate.kind
  );
}

function payloadSummary(candidate: Candidate): string {
  const payload = candidate.payload ?? {};
  if (candidate.kind === "knowledge_entry") {
    return `${payload.domainName ?? payload.domainKey ?? payload.domainId ?? "知识库"} · ${
      payload.summary ?? "等待作者核对字段与来源。"
    }`;
  }
  const values = [
    payload.description,
    payload.summary,
    payload.goal,
    payload.object,
    payload.contract,
    payload.type,
  ]
    .filter((value) => typeof value === "string" && value.trim())
    .map((value) => value.trim());
  return values[0] ?? "等待作者核对候选内容与证据。";
}

function sourceQuote(candidate: Candidate): string {
  const span = candidate.sourceSpan ?? "";
  const separator = span.lastIndexOf("｜");
  return separator >= 0 ? span.slice(separator + 1) : span;
}

function stepSummary(step: AgentStep): string {
  if (step.error) return step.error;
  const output = step.output ?? {};
  if (step.toolName === "project_inventory") {
    const counts = output.counts ?? {};
    return `${counts.entities ?? 0} 个实体 · ${counts.knowledgeEntries ?? 0} 条知识 · ${
      output.evidenceSources?.length ?? 0
    } 个可回查片段`;
  }
  if (step.toolName === "search_fts") {
    if (output.reason) return output.reason;
    return `${output.hits?.length ?? 0} 条检索命中 · 查询“${output.query ?? ""}”`;
  }
  if (step.toolName === "model_synthesis") {
    const created = output.created?.length ?? 0;
    const skipped = output.skipped?.length ?? 0;
    return `${output.summary ?? "已完成归纳"} · 新建 ${created} 项候选${
      skipped ? ` · 跳过 ${skipped} 项` : ""
    }`;
  }
  if (step.toolName === "foundation_check") {
    return output.ready
      ? `地基通过 ${output.passedCount}/${output.totalCount} 项检查`
      : `${output.errorCount ?? 0} 项阻断问题需要补齐`;
  }
  if (step.toolName === "candidate_impact") {
    return output.warningCount
      ? `${output.warningCount} 项候选存在冲突或依赖提醒`
      : `${output.candidateCount ?? 0} 项候选未发现新的影响警告`;
  }
  return step.status === "running" ? "正在执行…" : step.riskLevel;
}

function FoundationIcon({ check }: { check: FoundationCheck }) {
  if (check.severity === "error") {
    return <AlertTriangle size={12} className="mt-0.5 shrink-0 text-coral" />;
  }
  return <Check size={12} className="mt-0.5 shrink-0 text-teal" />;
}

export function AgentPage() {
  const project = useAppStore((state) => state.project);
  const setNav = useAppStore((state) => state.setNav);
  const notify = useAppStore((state) => state.notify);
  const refreshProject = useAppStore((state) => state.refreshProject);
  const [surface, setSurface] = useState<Surface>("chat");
  const [premise, setPremise] = useState<Premise | null>(null);
  const [runs, setRuns] = useState<AgentRun[]>([]);
  const [active, setActive] = useState<AgentRunDetail | null>(null);
  const [foundation, setFoundation] = useState<FoundationReport | null>(null);
  const [tools, setTools] = useState<AgentToolDefinition[]>([]);
  const [nextActions, setNextActions] = useState<AgentNextAction[]>([]);
  const [selectedStepId, setSelectedStepId] = useState<string | null>(null);
  const [inspecting, setInspecting] = useState<Candidate | null>(null);
  const [goal, setGoal] = useState(templates[0].goal);
  const [runMode, setRunMode] = useState("propose");
  const [scope, setScope] = useState("project");
  const [output, setOutput] = useState("all");
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  const loadRun = useCallback(async (id: string) => {
    const detail = await ipc.agentRunGet(id);
    setActive(detail);
    setSelectedStepId((current) =>
      detail.steps.some((step) => step.id === current)
        ? current
        : detail.steps.find((step) => step.status === "failed")?.id ??
          detail.steps.find((step) => step.status === "running")?.id ??
          detail.steps.at(-1)?.id ??
          null,
    );
    return detail;
  }, []);

  const refreshOverview = useCallback(async () => {
    const [nextRuns, nextFoundation, nextTools, actions] = await Promise.all([
      ipc.agentRuns(40),
      ipc.agentFoundation(),
      ipc.agentTools(),
      ipc.agentNextActions(),
    ]);
    setRuns(nextRuns);
    setFoundation(nextFoundation);
    setTools(nextTools);
    setNextActions(actions);
    return nextRuns;
  }, []);

  const refreshWorkflow = useCallback(async () => {
    const [premises] = await Promise.all([
      ipc.premiseList().catch(() => []),
      refreshOverview(),
      refreshProject(),
    ]);
    setPremise(premises[0] ?? null);
  }, [refreshOverview, refreshProject]);

  useEffect(() => {
    (async () => {
      try {
        void ipc
          .premiseList()
          .then((items) => setPremise(items[0] ?? null))
          .catch(() => setPremise(null));
        const nextRuns = await refreshOverview();
        if (nextRuns[0]) await loadRun(nextRuns[0].id);
      } catch (cause) {
        setError(cause instanceof Error ? cause.message : String(cause));
      } finally {
        setLoading(false);
      }
    })();
  }, [loadRun, refreshOverview]);

  useEffect(() => {
    let dispose = () => {};
    void onAgentProgress((event) => {
      if (active?.run.id && event.runId !== active.run.id) return;
      setProgress(`${event.message} · ${event.currentStep}/${event.totalSteps}`);
      if (event.runId) void loadRun(event.runId).catch(() => {});
    }).then((unlisten) => {
      dispose = unlisten;
    });
    return () => dispose();
  }, [active?.run.id, loadRun]);

  const execute = useCallback(
    async (id: string) => {
      setBusy(true);
      setError(null);
      setProgress("正在启动 Agent 执行循环");
      try {
        const detail = await ipc.agentRunExecute(id);
        setActive(detail);
        setSelectedStepId(
          detail.steps.find((step) => step.status === "failed")?.id ??
            detail.steps.at(-1)?.id ??
            null,
        );
        await refreshOverview();
        await refreshProject();
        if (detail.run.status === "waiting_approval") {
          notify("候选变更已进入审批区，权威资料尚未改变");
        } else if (detail.run.status === "completed") {
          notify("Agent 分析已完成");
        }
      } catch (cause) {
        const message = cause instanceof Error ? cause.message : String(cause);
        setError(message);
        try {
          await loadRun(id);
          await refreshOverview();
        } catch {
          // Keep the original execution error visible.
        }
      } finally {
        setBusy(false);
        setProgress(null);
      }
    },
    [loadRun, notify, refreshOverview, refreshProject],
  );

  const createAndRun = async () => {
    if (!goal.trim()) {
      setError("请先描述本次 Agent 任务目标。");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const input: AgentRunCreateInput = {
        goal: goal.trim(),
        mode: runMode,
        scope,
        output,
        sessionId: null,
      };
      const created = await ipc.agentRunCreate(input);
      setActive(created);
      await refreshOverview();
      await execute(created.run.id);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
      setBusy(false);
    }
  };

  const action = async (kind: "pause" | "resume" | "cancel", id: string) => {
    setError(null);
    try {
      const detail =
        kind === "pause"
          ? await ipc.agentRunPause(id)
          : kind === "resume"
            ? await ipc.agentRunResume(id)
            : await ipc.agentRunCancel(id);
      setActive(detail);
      await refreshOverview();
      if (kind === "resume") await execute(id);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    }
  };

  const retry = async (step: AgentStep) => {
    if (!active) return;
    setError(null);
    try {
      setActive(await ipc.agentRunRetryStep(active.run.id, step.id));
      await execute(active.run.id);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    }
  };

  const review = async (candidate: Candidate, accept: boolean) => {
    if (!active) return;
    setError(null);
    try {
      if (accept) await ipc.candidateAccept(candidate.id);
      else await ipc.candidateReject(candidate.id);
      await loadRun(active.run.id);
      await refreshOverview();
      await refreshProject();
      setInspecting(null);
      notify(accept ? "候选已确认并写入权威资料" : "候选已拒绝");
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    }
  };

  const selectedStep = useMemo(
    () => active?.steps.find((step) => step.id === selectedStepId) ?? null,
    [active, selectedStepId],
  );
  const pendingCandidates =
    active?.candidates.filter((candidate) => candidate.status === "pending") ?? [];
  const writeTools = tools.filter((tool) => tool.access === "write").length;
  const impactForInspecting =
    inspecting && active
      ? (active.steps
          .find((step) => step.toolName === "candidate_impact")
          ?.output?.items?.filter((item: any) => item.candidateId === inspecting.id) ?? [])
      : [];

  if (loading) {
    return (
      <div className="flex h-64 items-center justify-center text-[12px] text-muted">
        <LoaderCircle size={15} className="mr-2 animate-spin" /> 正在恢复 Agent 工作台
      </div>
    );
  }

  return (
    <div className="fade-in workspace-page px-6 py-5">
      <SectionTitle
        title="Agent 协作工作流"
        action={
          <div className="flex items-center gap-2">
            <div className="flex rounded-full border border-line bg-paper-raised p-0.5">
              <button
                onClick={() => setSurface("chat")}
                className={`rounded-full px-3 py-1.5 text-[10.5px] ${
                  surface === "chat"
                    ? "bg-teal text-paper-raised"
                    : "text-muted hover:text-ink"
                }`}
              >
                对话
              </button>
              <button
                onClick={() => setSurface("workspace")}
                className={`rounded-full px-3 py-1.5 text-[10.5px] ${
                  surface === "workspace"
                    ? "bg-teal text-paper-raised"
                    : "text-muted hover:text-ink"
                }`}
              >
                执行记录
              </button>
            </div>
            <Badge tone="teal">只读自动执行</Badge>
            <Badge tone="amber">写入进入候选</Badge>
          </div>
        }
      />

      {surface === "chat" ? (
        <ChatWorkspace
          premise={premise}
          surface="agent"
          nextActions={nextActions}
          onWorkflowComplete={refreshWorkflow}
        />
      ) : (
        <div className="workspace-fill grid grid-cols-[240px_minmax(0,1fr)_320px] gap-3">
          <aside className="paper-card flex h-full min-h-0 flex-col overflow-hidden">
            <div className="flex items-center justify-between border-b border-line px-3.5 py-3">
              <div>
                <div className="text-[11.5px] font-semibold text-ink">Run 历史</div>
                <div className="mt-0.5 text-[9.5px] text-muted">关闭项目后仍可恢复</div>
              </div>
              <button
                onClick={() => setActive(null)}
                className="focus-ring flex h-7 w-7 items-center justify-center rounded-lg border border-line text-muted hover:border-teal/40 hover:text-teal"
                title="新建任务"
              >
                <Plus size={13} />
              </button>
            </div>
            <div className="min-h-0 flex-1 overflow-y-auto p-1.5">
              {runs.length === 0 ? (
                <div className="px-3 py-10 text-center text-[10.5px] leading-5 text-muted">
                  还没有 Run。描述目标后，Agent 会先读取项目，再把写入内容送进候选区。
                </div>
              ) : (
                runs.map((run) => (
                  <button
                    key={run.id}
                    onClick={() => void loadRun(run.id)}
                    className={`mb-1 w-full rounded-lg px-2.5 py-2.5 text-left transition ${
                      active?.run.id === run.id ? "bg-teal-soft/70" : "hover:bg-ink/[0.035]"
                    }`}
                  >
                    <div className="flex items-start gap-2">
                      <span
                        className={`mt-1 h-1.5 w-1.5 shrink-0 rounded-full ${
                          ["running", "planning"].includes(run.status)
                            ? "animate-pulse bg-teal"
                            : run.status === "failed"
                              ? "bg-coral"
                              : run.status === "waiting_approval"
                                ? "bg-amber"
                                : "bg-muted/50"
                        }`}
                      />
                      <div className="min-w-0 flex-1">
                        <div className="line-clamp-2 text-[10.5px] font-medium leading-4 text-ink">
                          {run.goal}
                        </div>
                        <div className="mt-1 flex items-center gap-1.5 text-[8.5px] text-muted">
                          <span>{statusMeta[run.status]?.label ?? run.status}</span>
                          <span>·</span>
                          <span>{run.totalCandidates} 候选</span>
                          {run.pendingCandidates > 0 ? (
                            <span className="ml-auto rounded-full bg-amber-soft px-1.5 text-amber">
                              {run.pendingCandidates}
                            </span>
                          ) : null}
                        </div>
                      </div>
                    </div>
                  </button>
                ))
              )}
            </div>
            <div className="border-t border-line px-3.5 py-3">
              <div className="flex items-center justify-between text-[9.5px] text-muted">
                <span className="flex items-center gap-1.5">
                  <Workflow size={11} className="text-teal" />
                  {tools.length} 个注册工具
                </span>
                <span>{writeTools} 个写入</span>
              </div>
              <div className="mt-2 text-[9px] leading-4 text-muted/80">
                Agent 不能确认候选、提交正文或覆盖权威事实。
              </div>
            </div>
          </aside>

          <main className="flex h-full min-h-0 min-w-0 flex-col gap-3 overflow-hidden">
            <section className="paper-card shrink-0 overflow-hidden">
              <div className="border-b border-line px-4 py-3">
                <div className="flex items-center justify-between gap-3">
                  <div>
                    <div className="text-[12px] font-semibold text-ink">
                      {active ? "继续 Agent 任务" : "描述本次要推进的工作"}
                    </div>
                    <div className="mt-0.5 text-[9.5px] text-muted">
                      只读工具自动执行；任何设定、关系或骨架变更都先进入候选区。
                    </div>
                  </div>
                  <Badge tone={foundation?.ready ? "teal" : "amber"}>
                    地基 {foundation?.passedCount ?? 0}/{foundation?.totalCount ?? 7}
                  </Badge>
                </div>
              </div>
              <div className="px-4 py-3">
                <div className="mb-2 flex flex-wrap gap-1.5">
                  {templates.map((template) => (
                    <button
                      key={template.label}
                      onClick={() => {
                        setGoal(template.goal);
                        setOutput(template.output);
                        setRunMode(template.mode);
                        setActive(null);
                      }}
                      className="rounded-full border border-line bg-paper-raised px-2.5 py-1 text-[9.5px] text-ink-soft transition hover:border-teal/40 hover:text-teal"
                    >
                      {template.label}
                    </button>
                  ))}
                </div>
                <Textarea
                  rows={3}
                  value={goal}
                  onChange={(event) => setGoal(event.target.value)}
                  placeholder="例如：分析现有正文，整理人物、关系和伏笔候选，并说明对当前骨架的影响。"
                  className="min-h-[76px] text-[11.5px] leading-5"
                />
                <div className="mt-2 flex items-center gap-2">
                  <select
                    value={scope}
                    onChange={(event) => setScope(event.target.value)}
                    className="h-8 rounded-lg border border-line bg-paper-raised px-2 text-[10px] text-ink-soft"
                  >
                    {scopeOptions.map((option) => (
                      <option key={option.value} value={option.value}>
                        {option.label}
                      </option>
                    ))}
                  </select>
                  <select
                    value={output}
                    onChange={(event) => setOutput(event.target.value)}
                    className="h-8 rounded-lg border border-line bg-paper-raised px-2 text-[10px] text-ink-soft"
                  >
                    {outputOptions.map((option) => (
                      <option key={option.value} value={option.value}>
                        {option.label}
                      </option>
                    ))}
                  </select>
                  <select
                    value={runMode}
                    onChange={(event) => setRunMode(event.target.value)}
                    className="h-8 rounded-lg border border-line bg-paper-raised px-2 text-[10px] text-ink-soft"
                  >
                    <option value="propose">提出候选</option>
                    <option value="analyze">只分析</option>
                  </select>
                  <span className="ml-auto text-[9px] text-muted">人工审批</span>
                  <Button
                    size="sm"
                    tone="primary"
                    onClick={() => void createAndRun()}
                    disabled={busy || !goal.trim()}
                    icon={
                      busy ? (
                        <LoaderCircle size={12} className="animate-spin" />
                      ) : (
                        <Sparkles size={12} />
                      )
                    }
                  >
                    启动任务
                  </Button>
                </div>
                {error ? (
                  <div className="mt-2 flex items-start gap-2 rounded-lg bg-coral-soft/55 px-2.5 py-2 text-[10px] leading-5 text-coral">
                    <AlertTriangle size={12} className="mt-1 shrink-0" />
                    <span>{error}</span>
                  </div>
                ) : null}
                {progress ? (
                  <div className="mt-2 flex items-center gap-2 text-[9.5px] text-teal">
                    <LoaderCircle size={11} className="animate-spin" />
                    {progress}
                  </div>
                ) : null}
              </div>
            </section>

            <section className="paper-card flex min-h-0 flex-1 flex-col overflow-hidden">
              <div className="flex items-center justify-between gap-3 border-b border-line px-4 py-3">
                <div className="min-w-0">
                  <div className="flex items-center gap-2">
                    <span className="text-[11.5px] font-semibold text-ink">
                      {active ? "计划与执行轨迹" : "等待任务"}
                    </span>
                    {active ? statusBadge(active.run.status) : null}
                  </div>
                  <div className="mt-0.5 truncate text-[9.5px] text-muted">
                    {active
                      ? active.run.goal
                      : "启动后，每个工具结果、证据和失败原因都会落盘到当前项目。"}
                  </div>
                </div>
                {active ? (
                  <div className="flex shrink-0 items-center gap-1">
                    {["running", "planning"].includes(active.run.status) ? (
                      <Button
                        size="sm"
                        tone="ghost"
                        onClick={() => void action("pause", active.run.id)}
                        icon={<CirclePause size={12} />}
                      >
                        暂停
                      </Button>
                    ) : null}
                    {active.run.status === "paused" ? (
                      <Button
                        size="sm"
                        tone="ghost"
                        onClick={() => void action("resume", active.run.id)}
                        icon={<CirclePlay size={12} />}
                      >
                        恢复
                      </Button>
                    ) : null}
                    {!["completed", "canceled"].includes(active.run.status) ? (
                      <Button
                        size="sm"
                        tone="ghost"
                        onClick={() => void action("cancel", active.run.id)}
                        icon={<Square size={11} />}
                      >
                        取消
                      </Button>
                    ) : null}
                  </div>
                ) : null}
              </div>
              <div className="min-h-0 flex-1 overflow-y-auto">
                {!active ? (
                  <div className="flex h-full min-h-[220px] flex-col items-center justify-center px-8 text-center">
                    <div className="mb-3 flex h-10 w-10 items-center justify-center rounded-full border border-teal/20 bg-teal-soft/40 text-teal">
                      <ListChecks size={16} />
                    </div>
                    <div className="text-[13px] font-medium text-ink">
                      从任务目标开始，不从工具按钮开始
                    </div>
                    <p className="mt-1.5 max-w-md text-[10.5px] leading-5 text-muted">
                      Agent 会读取项目、检索原文、生成证据约束候选，并在写入权威资料前停下等待你的审批。
                    </p>
                  </div>
                ) : (
                  <div className="p-3">
                    <div className="space-y-1">
                      {active.steps.map((step) => {
                        const meta = stepMeta[step.toolName] ?? {
                          label: step.toolName,
                          icon: Workflow,
                        };
                        const Icon = meta.icon;
                        const selected = selectedStep?.id === step.id;
                        return (
                          <button
                            key={step.id}
                            onClick={() => setSelectedStepId(step.id)}
                            className={`focus-ring flex w-full items-start gap-2.5 rounded-xl border px-3 py-2.5 text-left transition ${
                              selected
                                ? "border-teal/25 bg-teal-soft/35"
                                : "border-transparent hover:border-line hover:bg-ink/[0.025]"
                            }`}
                          >
                            <div
                              className={`mt-0.5 flex h-7 w-7 shrink-0 items-center justify-center rounded-lg ${
                                step.status === "failed"
                                  ? "bg-coral-soft text-coral"
                                  : step.status === "completed"
                                    ? "bg-teal-soft text-teal"
                                    : step.status === "running"
                                      ? "bg-teal text-paper-raised"
                                      : "bg-ink/5 text-muted"
                              }`}
                            >
                              {step.status === "running" ? (
                                <LoaderCircle size={13} className="animate-spin" />
                              ) : (
                                <Icon size={13} />
                              )}
                            </div>
                            <div className="min-w-0 flex-1">
                              <div className="flex items-center gap-2">
                                <span className="text-[10.5px] font-medium text-ink">
                                  {step.seq}. {meta.label}
                                </span>
                                <span className="text-[8.5px] uppercase tracking-wide text-muted">
                                  {step.status}
                                </span>
                                {step.requiresApproval ? (
                                  <Badge tone="amber">候选边界</Badge>
                                ) : null}
                              </div>
                              <div className="mt-1 line-clamp-2 text-[9.5px] leading-4 text-muted">
                                {stepSummary(step)}
                              </div>
                            </div>
                            <ChevronRight size={12} className="mt-1 shrink-0 text-muted/60" />
                          </button>
                        );
                      })}
                    </div>

                    {selectedStep ? (
                      <div className="mt-3 rounded-xl border border-line bg-paper px-3.5 py-3">
                        <div className="flex items-start justify-between gap-3">
                          <div>
                            <div className="text-[10.5px] font-semibold text-ink">
                              步骤观察 ·{" "}
                              {stepMeta[selectedStep.toolName]?.label ?? selectedStep.toolName}
                            </div>
                            <div className="mt-1 text-[10px] leading-5 text-ink-soft">
                              {stepSummary(selectedStep)}
                            </div>
                          </div>
                          {selectedStep.status === "failed" ? (
                            <Button
                              size="sm"
                              tone="ghost"
                              onClick={() => void retry(selectedStep)}
                              icon={<RotateCcw size={11} />}
                            >
                              重试此步
                            </Button>
                          ) : null}
                        </div>
                        {selectedStep.toolName === "model_synthesis" &&
                        selectedStep.output?.skipped?.length ? (
                          <div className="mt-2 space-y-1 border-t border-line pt-2">
                            {selectedStep.output.skipped
                              .slice(0, 4)
                              .map((item: any, index: number) => (
                                <div
                                  key={`${item.reason}-${index}`}
                                  className="text-[9px] leading-4 text-amber"
                                >
                                  跳过 {item.title ?? item.kind ?? "候选"}：{item.reason}
                                </div>
                              ))}
                          </div>
                        ) : null}
                        {selectedStep.toolName === "candidate_impact" &&
                        selectedStep.output?.items?.length ? (
                          <div className="mt-2 space-y-1.5 border-t border-line pt-2">
                            {selectedStep.output.items
                              .slice(0, 5)
                              .map((item: any, index: number) => (
                                <div
                                  key={`${item.candidateId}-${index}`}
                                  className="text-[9px] leading-4 text-amber"
                                >
                                  {item.title ?? "候选"}：{item.message}
                                </div>
                              ))}
                          </div>
                        ) : null}
                        {active?.evidence.filter((item) => item.stepId === selectedStep.id).length ? (
                          <div className="mt-2 space-y-1.5 border-t border-line pt-2">
                            <div className="text-[8.5px] font-medium uppercase tracking-wide text-muted">
                              Run 证据
                            </div>
                            {active.evidence
                              .filter((item) => item.stepId === selectedStep.id)
                              .slice(0, 4)
                              .map((item) => (
                                <div key={item.id} className="rounded-lg bg-ink/[0.035] px-2 py-1.5">
                                  <div className="text-[8px] text-muted">
                                    {item.sourceKind}:{item.sourceId}
                                  </div>
                                  <div className="mt-0.5 line-clamp-2 text-[8.5px] leading-4 text-ink-soft">
                                    {item.quote}
                                  </div>
                                </div>
                              ))}
                          </div>
                        ) : null}
                        {selectedStep.error ? (
                          <div className="mt-2 rounded-lg bg-coral-soft/55 px-2.5 py-2 text-[9.5px] leading-4 text-coral">
                            {selectedStep.error}
                          </div>
                        ) : null}
                      </div>
                    ) : null}
                  </div>
                )}
              </div>
            </section>
          </main>

          <aside className="flex h-full min-h-0 flex-col gap-3 overflow-hidden">
            <section className="paper-card shrink-0 overflow-hidden">
              <div className="flex items-center justify-between border-b border-line px-3.5 py-3">
                <div>
                  <div className="text-[11px] font-semibold text-ink">项目健康</div>
                  <div className="mt-0.5 text-[9px] text-muted">卷章规划门槛</div>
                </div>
                <Badge tone={foundation?.ready ? "teal" : "coral"}>
                  {foundation?.ready ? "可规划" : `${foundation?.errorCount ?? 0} 阻断`}
                </Badge>
              </div>
              <div className="space-y-2 px-3.5 py-3">
                {(foundation?.checks ?? []).slice(0, 6).map((check) => (
                  <div key={check.code} className="flex items-start gap-2">
                    <FoundationIcon check={check} />
                    <div className="min-w-0">
                      <div className="text-[9.5px] font-medium text-ink-soft">{check.label}</div>
                      <div className="mt-0.5 line-clamp-2 text-[8.5px] leading-3.5 text-muted">
                        {check.message}
                      </div>
                    </div>
                  </div>
                ))}
              </div>
              <button
                onClick={() => setNav("seed")}
                className="flex w-full items-center justify-between border-t border-line px-3.5 py-2 text-[9.5px] text-teal hover:bg-teal-soft/25"
              >
                查看叙事地基
                <ChevronRight size={11} />
              </button>
            </section>

            <section className="paper-card flex min-h-0 flex-1 flex-col overflow-hidden">
              <div className="flex items-center justify-between border-b border-line px-3.5 py-3">
                <div>
                  <div className="text-[11px] font-semibold text-ink">待审批变更</div>
                  <div className="mt-0.5 text-[9px] text-muted">
                    {pendingCandidates.length} 项尚未进入权威资料
                  </div>
                </div>
                <Clock3 size={13} className="text-amber" />
              </div>
              <div className="min-h-0 flex-1 overflow-y-auto">
                {pendingCandidates.length === 0 ? (
                  <div className="flex h-full min-h-[180px] flex-col items-center justify-center px-6 text-center">
                    <ShieldCheck size={18} className="mb-2 text-teal/60" />
                    <div className="text-[10.5px] font-medium text-ink-soft">没有待审批候选</div>
                    <div className="mt-1 text-[9px] leading-4 text-muted">
                      Agent 只能提出变更；接受后才进入事实表。
                    </div>
                  </div>
                ) : (
                  pendingCandidates.map((candidate) => (
                    <article
                      key={candidate.id}
                      className="border-b border-line px-3.5 py-3 last:border-0"
                    >
                      <div className="flex items-start gap-2">
                        <div className="min-w-0 flex-1">
                          <div className="text-[9.5px] font-semibold text-ink">
                            {payloadTitle(candidate)}
                          </div>
                          <div className="mt-1 line-clamp-2 text-[9px] leading-4 text-muted">
                            {payloadSummary(candidate)}
                          </div>
                        </div>
                        <Badge tone={candidate.riskLevel === "L3" ? "coral" : "neutral"}>
                          {candidate.riskLevel}
                        </Badge>
                      </div>
                      <div className="mt-2 rounded-lg bg-ink/[0.035] px-2 py-1.5">
                        <div className="text-[8px] font-medium uppercase tracking-wide text-muted">
                          {kindLabels[candidate.kind] ?? candidate.kind}
                        </div>
                        <blockquote className="mt-1 line-clamp-3 text-[8.5px] leading-4 text-ink-soft">
                          “{sourceQuote(candidate)}”
                        </blockquote>
                      </div>
                      <div className="mt-2 flex items-center justify-between">
                        <span className="text-[8.5px] text-muted">
                          置信度 {Math.round((candidate.confidence ?? 0) * 100)}%
                        </span>
                        <div className="flex gap-1">
                          <button
                            onClick={() => setInspecting(candidate)}
                            className="focus-ring flex h-6 items-center rounded-md px-1.5 text-[8.5px] text-muted hover:bg-ink/5 hover:text-ink"
                            title="查看候选差异"
                          >
                            差异
                          </button>
                          <button
                            onClick={() => void review(candidate, false)}
                            className="focus-ring flex h-6 w-6 items-center justify-center rounded-md text-muted hover:bg-coral-soft hover:text-coral"
                            title="拒绝候选"
                          >
                            <X size={11} />
                          </button>
                          <button
                            onClick={() => void review(candidate, true)}
                            disabled={candidate.riskLevel === "L3"}
                            className="focus-ring flex h-6 w-6 items-center justify-center rounded-md text-teal hover:bg-teal-soft disabled:cursor-not-allowed disabled:text-muted/40"
                            title={
                              candidate.riskLevel === "L3"
                                ? "L3 硬冲突需先到审计页处理"
                                : "确认候选"
                            }
                          >
                            <Check size={12} />
                          </button>
                        </div>
                      </div>
                    </article>
                  ))
                )}
              </div>
            </section>

            <section className="paper-card shrink-0 px-3.5 py-3">
              <div className="flex items-center justify-between">
                <div className="text-[9.5px] font-medium text-ink-soft">任务状态</div>
                <MessageSquare size={11} className="text-muted" />
              </div>
              <div className="mt-1.5 text-[8.5px] leading-4 text-muted">
                {project
                  ? `${project.counts.pendingCandidates} 个候选待处理 · ${project.counts.openIssues} 个冲突`
                  : "本地项目"}
              </div>
            </section>
          </aside>
        </div>
      )}
      <Modal
        open={Boolean(inspecting)}
        onOpenChange={(open) => {
          if (!open) setInspecting(null);
        }}
        title={inspecting ? `核对候选 · ${payloadTitle(inspecting)}` : "核对候选"}
        description="确认后会写入权威资料；拒绝不会改变已确认内容。"
        width="max-w-[720px]"
        footer={
          inspecting ? (
            <>
              <Button
                size="sm"
                onClick={() => void review(inspecting, false)}
                icon={<X size={12} />}
              >
                拒绝
              </Button>
              <Button
                size="sm"
                tone="primary"
                disabled={inspecting.riskLevel === "L3"}
                onClick={() => void review(inspecting, true)}
                icon={<Check size={12} />}
              >
                确认并写入
              </Button>
            </>
          ) : null
        }
      >
        {inspecting ? (
          <div className="space-y-4">
            <div className="grid grid-cols-[1fr_auto] gap-3 rounded-xl border border-line bg-paper px-3.5 py-3">
              <div>
                <div className="text-[9px] uppercase tracking-wide text-muted">
                  {kindLabels[inspecting.kind] ?? inspecting.kind}
                </div>
                <div className="mt-1 text-[12px] font-medium text-ink">
                  {payloadTitle(inspecting)}
                </div>
                <div className="mt-1 text-[10px] leading-5 text-ink-soft">
                  {payloadSummary(inspecting)}
                </div>
              </div>
              <div className="text-right">
                <Badge tone={inspecting.riskLevel === "L3" ? "coral" : "neutral"}>
                  {inspecting.riskLevel}
                </Badge>
                <div className="mt-1 text-[8.5px] text-muted">
                  置信度 {Math.round((inspecting.confidence ?? 0) * 100)}%
                </div>
              </div>
            </div>

            <div>
              <div className="mb-1.5 text-[9.5px] font-medium tracking-wide text-muted">来源证据</div>
              <div className="rounded-xl border border-line bg-paper-raised px-3.5 py-3">
                <div className="text-[8.5px] text-muted">{inspecting.sourceSpan}</div>
                <blockquote className="mt-1.5 text-[11px] leading-6 text-ink-soft">
                  “{sourceQuote(inspecting)}”
                </blockquote>
              </div>
            </div>

            {impactForInspecting.length ? (
              <div>
                <div className="mb-1.5 text-[9.5px] font-medium tracking-wide text-muted">
                  冲突与影响
                </div>
                <div className="space-y-1.5">
                  {impactForInspecting.map((item: any, index: number) => (
                    <div
                      key={`${item.candidateId}-${index}`}
                      className="rounded-lg bg-amber-soft/40 px-3 py-2 text-[10px] leading-5 text-amber"
                    >
                      {item.message}
                    </div>
                  ))}
                </div>
              </div>
            ) : null}

            <div>
              <div className="mb-1.5 text-[9.5px] font-medium tracking-wide text-muted">
                结构化变更
              </div>
              <pre className="max-h-[300px] overflow-auto rounded-xl border border-line bg-paper px-3.5 py-3 font-mono text-[10px] leading-5 text-ink-soft">
                {JSON.stringify(inspecting.payload, null, 2)}
              </pre>
            </div>
            <div className="text-[9px] leading-4 text-muted">
              Run：{inspecting.agentRunId ?? "未关联"} · Step：{inspecting.agentStepId ?? "未关联"}
            </div>
          </div>
        ) : null}
      </Modal>
    </div>
  );
}
