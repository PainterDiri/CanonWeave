import { useCallback, useEffect, useMemo, useState } from "react";
import {
  Check,
  ChevronRight,
  CircleAlert,
  ClipboardCheck,
  LoaderCircle,
  ShieldCheck,
  X,
} from "lucide-react";
import { Badge, Button, Modal, SectionTitle } from "@/components/ui";
import { summaryOf } from "@/features/seed/ChatWorkspace";
import {
  ipc,
  type AuditIssue,
  type BatchPreview,
  type ReviewPolicy,
  type ReviewQueue,
} from "@/lib/ipc";
import { useAppStore } from "@/stores/ui";

const kindOptions = [
  { key: "", label: "全部类型" },
  { key: "entity", label: "实体" },
  { key: "claim", label: "事实" },
  { key: "event", label: "事件" },
  { key: "relation", label: "关系" },
  { key: "promise", label: "伏笔" },
  { key: "rule", label: "规则" },
  { key: "style_sample", label: "文笔样本" },
  { key: "scene_goal", label: "章节目标" },
  { key: "knowledge_entry", label: "知识条目" },
  { key: "outline_patch", label: "骨架补丁" },
];

export function AuditPage() {
  const [queue, setQueue] = useState<ReviewQueue | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const [kind, setKind] = useState("");
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [batch, setBatch] = useState<BatchPreview | null>(null);
  const [policy, setPolicy] = useState<ReviewPolicy | null>(null);
  const [issues, setIssues] = useState<AuditIssue[]>([]);
  const [lastBatch, setLastBatch] = useState<string | null>(null);
  const [policyPreview, setPolicyPreview] = useState<string[] | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const refreshProject = useAppStore((s) => s.refreshProject);

  const load = useCallback(async () => {
    try {
      const [result, currentPolicy, issueList] = await Promise.all([
        ipc.reviewQueue(),
        ipc.reviewPolicyGet(),
        ipc.issues("open"),
      ]);
      setQueue(result);
      setPolicy(currentPolicy);
      setIssues(issueList);
      setSelected((current) =>
        current && result.candidates.some((c) => c.id === current)
          ? current
          : (result.candidates[0]?.id ?? null),
      );
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const filtered = useMemo(
    () => (queue?.candidates ?? []).filter((candidate) => !kind || candidate.kind === kind),
    [queue, kind],
  );
  const current = filtered.find((candidate) => candidate.id === selected) ?? filtered[0] ?? null;

  const accept = useCallback(
    async (id: string) => {
      setBusy(true);
      try {
        await ipc.candidateAccept(id);
        await load();
        await refreshProject();
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
      } finally {
        setBusy(false);
      }
    },
    [load, refreshProject],
  );

  const reject = useCallback(
    async (id: string) => {
      setBusy(true);
      try {
        await ipc.candidateReject(id);
        await load();
        await refreshProject();
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
      } finally {
        setBusy(false);
      }
    },
    [load, refreshProject],
  );

  const previewBatch = async () => {
    setBusy(true);
    try {
      setBatch(await ipc.reviewBatchPreview(null));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const applyBatch = async () => {
    if (!batch) return;
    setBusy(true);
    try {
      const result = await ipc.reviewBatchApply(batch.ids, null);
      setBatch(null);
      setLastBatch(result.batchId);
      await load();
      await refreshProject();
      setNotice(`已批量接受 ${result.accepted} 条 L1，生成 revision ${result.revisionId.slice(0, 8)}`);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const changePolicy = async (mode: string) => {
    setBusy(true);
    try {
      const saved = await ipc.reviewPolicySave({ scope: "project", scopeId: null, mode });
      setPolicy(saved);
      setNotice(`审核策略已切换为${policyLabel(mode)}，切换已记录`);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const applyPolicy = async () => {
    setBusy(true);
    try {
      const result = await ipc.reviewPolicyApply(null);
      setPolicyPreview(null);
      setLastBatch(result.batchId);
      setNotice(result.message);
      await load();
      await refreshProject();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const previewPolicy = () => {
    if (!policy || !queue) return;
    const levels =
      policy.mode === "fast"
        ? ["L0", "L1"]
        : policy.mode === "balanced"
          ? ["L0"]
          : [];
    setPolicyPreview(
      queue.candidates
        .filter((candidate) => levels.includes(candidate.riskLevel))
        .map((candidate) => candidate.id),
    );
  };

  const undoBatch = async () => {
    if (!lastBatch) return;
    setBusy(true);
    try {
      const result = await ipc.reviewBatchUndo(lastBatch);
      setNotice(`整批撤销成功，恢复 ${result.restored} 条候选`);
      setLastBatch(null);
      await load();
      await refreshProject();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const runAudit = async () => {
    setBusy(true);
    try {
      const result = await ipc.auditRun();
      setNotice(`审计完成：新增 ${result.issuesCreated} 个问题`);
      await load();
      await refreshProject();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const actOnIssue = async (issue: AuditIssue, action: string) => {
    const reason =
      action.startsWith("exception")
        ? window.prompt("记录例外原因；例外的规则指纹会一并保存") ?? ""
        : "";
    if (action.startsWith("exception") && !reason.trim()) return;
    setBusy(true);
    try {
      await ipc.issueAction({ id: issue.id, action, reason });
      await load();
      await refreshProject();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement;
      if (["INPUT", "TEXTAREA", "SELECT"].includes(target.tagName)) return;
      if (!current || busy) return;
      const index = filtered.findIndex((c) => c.id === current.id);
      if (event.key === "j" || event.key === "ArrowDown") {
        event.preventDefault();
        setSelected(filtered[Math.min(index + 1, filtered.length - 1)]?.id ?? null);
      } else if (event.key === "k" || event.key === "ArrowUp") {
        event.preventDefault();
        setSelected(filtered[Math.max(index - 1, 0)]?.id ?? null);
      } else if (event.key === "a") {
        event.preventDefault();
        void accept(current.id);
      } else if (event.key === "r") {
        event.preventDefault();
        void reject(current.id);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [current, filtered, busy, accept, reject]);

  const groups = [
    { key: "L3", label: "必须处理", tone: "coral" as const, hint: "硬冲突在任何策略下都必须人工处理" },
    { key: "L2", label: "建议确认", tone: "amber" as const, hint: "涉及关系、死亡、时间、持有物或伏笔" },
    { key: "L1", label: "可批量确认", tone: "teal" as const, hint: "有原文引用、无冲突、影响范围小" },
  ];

  return (
    <div className="fade-in workspace-page px-8 py-7">
      <SectionTitle
        eyebrow="Audit"
        title="一致性审计"
        action={
          <div className="flex items-center gap-2">
            <Button
              size="sm"
              onClick={() => void runAudit()}
              disabled={busy}
            >
              运行审计
            </Button>
            <select
              value={policy?.mode ?? "balanced"}
              onChange={(e) => void changePolicy(e.target.value)}
              disabled={busy}
              className="h-8 rounded-lg border border-line bg-paper-raised px-2 text-[10.5px] text-ink-soft"
            >
              <option value="strict">严格</option>
              <option value="balanced">平衡</option>
              <option value="fast">快速</option>
              <option value="draft">草稿</option>
            </select>
            <Button
              size="sm"
              tone="primary"
              onClick={() => previewPolicy()}
              disabled={busy}
            >
              按策略处理
            </Button>
            <Button
              size="sm"
              onClick={() => void previewBatch()}
              disabled={busy || !queue?.counts.l1}
            >
              预览 L1
            </Button>
            {lastBatch ? (
              <Button size="sm" tone="danger" onClick={() => void undoBatch()} disabled={busy}>
                撤销整批
              </Button>
            ) : null}
          </div>
        }
      />

      {notice ? (
        <div className="mb-4 rounded-lg border border-teal/20 bg-teal-soft/50 px-4 py-2.5 text-[11.5px] leading-5 text-teal">
          {notice}
        </div>
      ) : null}

      {error ? (
        <div className="mb-4 rounded-lg border border-coral/25 bg-coral-soft/60 px-4 py-2.5 text-[12px] text-coral">
          {error}
        </div>
      ) : null}

      <div className="mb-4 grid grid-cols-3 gap-3">
        {groups.map((group) => {
          const count = queue?.candidates.filter((c) => c.riskLevel === group.key).length ?? 0;
          return (
            <div key={group.key} className="paper-card flex items-center justify-between px-4 py-3.5">
              <div>
                <div className="flex items-center gap-2">
                  <span className="text-[12px] font-semibold text-ink">{group.label}</span>
                  <Badge tone={group.tone}>{group.key}</Badge>
                </div>
                <div className="mt-1 text-[10px] text-muted">{group.hint}</div>
              </div>
              <span className="display text-[24px] text-ink">{count}</span>
            </div>
          );
        })}
      </div>

      {issues.length > 0 ? (
        <section className="mb-4 space-y-2">
          <div className="flex items-center justify-between px-1">
            <div className="text-[10px] font-semibold uppercase tracking-[0.16em] text-coral">
              Conflict certificates · {issues.length}
            </div>
            <span className="text-[9.5px] text-muted">
              每个问题都带规则、证据与可执行处理
            </span>
          </div>
          {issues.slice(0, 5).map((issue) => (
            <article
              key={issue.id}
              className={`paper-card px-5 py-4 ${issue.riskLevel === "L3" ? "border-coral/30" : "border-amber/25"}`}
            >
              <div className="flex items-start justify-between gap-4">
                <div className="min-w-0">
                  <div className="flex items-center gap-2">
                    <CircleAlert
                      size={13}
                      className={issue.riskLevel === "L3" ? "text-coral" : "text-amber"}
                    />
                    <span className="text-[11px] font-semibold text-ink">{issue.ruleCode}</span>
                    <Badge tone={issue.riskLevel === "L3" ? "coral" : "amber"}>
                      {issue.riskLevel}
                    </Badge>
                    <span className="text-[9px] text-muted">{issue.constraintLayer}</span>
                  </div>
                  <p className="mt-1.5 max-w-3xl text-[11.5px] leading-5 text-ink-soft">
                    {issue.explanation}
                  </p>
                  <div className="mt-1.5 flex flex-wrap gap-1">
                    {issue.evidenceSpans.map((span) => (
                      <span
                        key={span}
                        className="max-w-[280px] truncate rounded bg-paper px-1.5 py-0.5 font-mono text-[9px] text-muted"
                        title={span}
                      >
                        {span}
                      </span>
                    ))}
                  </div>
                  {issue.solverCore.length ? (
                    <div className="mt-2 flex flex-wrap items-center gap-1">
                      <span className="text-[9px] font-medium text-coral">冲突核心</span>
                      {issue.solverCore.map((item) => (
                        <span
                          key={item}
                          className="rounded border border-coral/20 bg-coral-soft/40 px-1.5 py-0.5 font-mono text-[8.5px] text-coral"
                        >
                          {item}
                        </span>
                      ))}
                    </div>
                  ) : null}
                </div>
                <div className="flex shrink-0 flex-wrap justify-end gap-1">
                  <Button size="sm" tone="ghost" onClick={() => void actOnIssue(issue, "defer")}>
                    稍后
                  </Button>
                  <Button size="sm" onClick={() => void actOnIssue(issue, "exception_once")}>
                    本次例外
                  </Button>
                  {issue.riskLevel !== "L3" ? (
                    <Button
                      size="sm"
                      tone="primary"
                      onClick={() => void actOnIssue(issue, "resolve")}
                    >
                      已修复
                    </Button>
                  ) : null}
                </div>
              </div>
            </article>
          ))}
        </section>
      ) : null}

      {loading ? (
        <div className="flex h-64 items-center justify-center text-[12px] text-muted">
          <LoaderCircle size={15} className="mr-2 animate-spin" /> 正在读取审核队列
        </div>
      ) : queue && queue.candidates.length === 0 ? (
        <div className="paper-card flex flex-col items-center justify-center px-8 py-16 text-center">
          <div className="mb-4 flex h-12 w-12 items-center justify-center rounded-full border border-line bg-paper text-teal">
            <ShieldCheck size={20} />
          </div>
          <div className="display text-[19px] text-ink">没有待处理候选</div>
          <p className="mt-2 max-w-md text-[12px] leading-6 text-muted">
            抽取和对话产生的未确认内容会进入这里。L1 可批量确认，L3 硬冲突永远需要人工处理。
          </p>
        </div>
      ) : (
        <div className="workspace-fill grid min-h-[520px] grid-cols-[360px_minmax(0,1fr)] gap-4">
          <section className="paper-card flex h-full min-h-0 flex-col overflow-hidden">
            <div className="flex items-center justify-between border-b border-line px-3 py-2.5">
              <span className="text-[11px] font-semibold text-ink-soft">
                待确认 {filtered.length}
              </span>
              <select
                value={kind}
                onChange={(e) => setKind(e.target.value)}
                className="rounded-md border border-line bg-paper px-2 py-1 text-[10px] text-ink-soft outline-none"
              >
                {kindOptions.map((option) => (
                  <option key={option.key} value={option.key}>
                    {option.label}
                  </option>
                ))}
              </select>
            </div>
            <div className="min-h-0 flex-1 overflow-y-auto p-1.5">
              {filtered.map((candidate) => (
                <button
                  key={candidate.id}
                  onClick={() => setSelected(candidate.id)}
                  className={`mb-1 w-full rounded-lg border px-3 py-2.5 text-left transition ${
                    current?.id === candidate.id
                      ? "border-teal/25 bg-teal-soft/60"
                      : "border-transparent hover:bg-ink/[0.03]"
                  }`}
                >
                  <div className="flex items-center gap-2">
                    <RiskDot risk={candidate.riskLevel} />
                    <span className="text-[10px] font-medium uppercase tracking-wide text-muted">
                      {labelOf(candidate.kind)}
                    </span>
                    <span className="ml-auto text-[9px] text-muted">{candidate.riskLevel}</span>
                  </div>
                  <div className="mt-1.5 line-clamp-2 text-[10.5px] leading-5 text-ink-soft">
                    {summaryOf(candidate)}
                  </div>
                </button>
              ))}
            </div>
            <div className="border-t border-line px-3 py-1.5 text-[9px] text-muted">
              J/K 移动 · A 接受 · R 拒绝
            </div>
          </section>

          {current ? (
            <section className="paper-card flex h-full min-h-0 flex-col overflow-hidden">
              <div className="flex items-start justify-between border-b border-line px-5 py-4">
                <div>
                  <div className="flex items-center gap-2">
                    <RiskDot risk={current.riskLevel} />
                    <span className="text-[11px] font-semibold text-ink">
                      {labelOf(current.kind)}候选
                    </span>
                    <Badge tone={current.riskLevel === "L2" ? "amber" : "teal"}>
                      {current.riskLevel}
                    </Badge>
                  </div>
                  <p className="mt-2 max-w-xl text-[12.5px] leading-6 text-ink">
                    {summaryOf(current)}
                  </p>
                </div>
              </div>
              <div className="min-h-0 flex-1 overflow-y-auto px-5 py-4">
                <div className="mb-4 grid grid-cols-2 gap-3 text-[10.5px]">
                  <Info label="来源证据" value={current.sourceSpan ?? "无原文引用（升级为 L2）"} />
                  <Info
                    label="置信度"
                    value={`${Math.round((current.confidence ?? 0) * 100)}% · 只作参考`}
                  />
                </div>
                <div className="mb-2 flex items-center gap-1.5 text-[10px] font-medium tracking-wide text-muted">
                  <ClipboardCheck size={11} />
                  结构化内容
                </div>
                <pre className="max-h-[260px] overflow-auto rounded-lg border border-line bg-paper px-3.5 py-3 font-mono text-[10.5px] leading-5 text-ink-soft">
                  {JSON.stringify(current.payload, null, 2)}
                </pre>
                {current.riskLevel === "L2" ? (
                  <div className="mt-4 flex items-start gap-2 rounded-lg border border-amber/25 bg-amber-soft/40 px-3 py-2 text-[10.5px] leading-5 text-amber">
                    <CircleAlert size={13} className="mt-0.5 shrink-0" />
                    该条涉及高影响事实，确认后会写入权威库；请先核对原文证据。
                  </div>
                ) : null}
              </div>
              <div className="flex items-center justify-between border-t border-line px-5 py-3">
                <span className="text-[9.5px] text-muted">
                  接受后会记录策略、批次与时间，可在审计日志中追溯
                </span>
                <div className="flex gap-2">
                  <Button
                    size="sm"
                    onClick={() => void reject(current.id)}
                    disabled={busy}
                    icon={<X size={12} />}
                  >
                    拒绝
                  </Button>
                  <Button
                    size="sm"
                    tone="primary"
                    onClick={() => void accept(current.id)}
                    disabled={busy || current.riskLevel === "L3"}
                    icon={busy ? <LoaderCircle size={12} className="animate-spin" /> : <Check size={12} />}
                  >
                    确认并写入
                  </Button>
                </div>
              </div>
            </section>
          ) : (
            <div className="paper-card flex items-center justify-center text-[12px] text-muted">
              选择左侧条目查看证据
              <ChevronRight size={14} className="ml-1" />
            </div>
          )}
        </div>
      )}

      {policy ? (
        <div className="mt-3 flex items-center gap-3 rounded-xl border border-line bg-paper-raised px-4 py-2.5 text-[9.5px] text-muted">
          <span>
            当前：{policyLabel(policy.mode)} · 自动通过 {policy.autoAcceptLevel}
          </span>
          <span>抽样 {policy.sampleRate}%</span>
          <span>L3 永不自动通过</span>
          <span className="ml-auto">切换策略会写入审计日志</span>
        </div>
      ) : null}

      <Modal
        open={Boolean(batch)}
        onOpenChange={(open) => !open && setBatch(null)}
        title="批量接受低风险候选"
        description="只会处理 L1。L2/L3 仍留在队列中逐条确认；接受后生成单一 revision。"
        footer={
          <>
            <Button tone="ghost" onClick={() => setBatch(null)}>
              取消
            </Button>
            <Button
              tone="primary"
              onClick={() => void applyBatch()}
              disabled={busy || !batch?.count}
            >
              接受 {batch?.count ?? 0} 条
            </Button>
          </>
        }
      >
        {batch ? (
          <div className="space-y-3">
            <div className="grid grid-cols-3 gap-2">
              {[
                ["可接受 L1", batch.l1],
                ["需逐条 L2", batch.l2],
                ["必须处理 L3", batch.l3],
              ].map(([label, value]) => (
                <div key={String(label)} className="rounded-lg border border-line bg-paper px-3 py-2.5">
                  <div className="text-[9.5px] text-muted">{label}</div>
                  <div className="mt-1 text-[20px] text-ink">{value}</div>
                </div>
              ))}
            </div>
            <div className="rounded-lg border border-line bg-paper/60 px-3 py-2.5 text-[10.5px] leading-5 text-muted">
              本批共 {batch.count} 条 L1 会写入权威库；{batch.blocked.length} 条高影响候选不会自动通过。
              接受后记录批次、策略与时间。
            </div>
          </div>
        ) : null}
      </Modal>

      <Modal
        open={policyPreview !== null}
        onOpenChange={(open) => !open && setPolicyPreview(null)}
        title="按审核策略预览"
        description="只预览会被当前策略自动接受的低风险条目；L2/L3 不会进入批次。"
        footer={
          <>
            <Button tone="ghost" onClick={() => setPolicyPreview(null)}>
              取消
            </Button>
            <Button
              tone="primary"
              onClick={() => void applyPolicy()}
              disabled={busy || !policyPreview?.length}
            >
              接受 {policyPreview?.length ?? 0} 条
            </Button>
          </>
        }
      >
        <div className="space-y-3">
          <div className="grid grid-cols-2 gap-2">
            <div className="rounded-lg border border-line bg-paper px-3 py-2.5">
              <div className="text-[9.5px] text-muted">当前策略</div>
              <div className="mt-1 text-[14px] text-ink">{policyLabel(policy?.mode ?? "")}</div>
            </div>
            <div className="rounded-lg border border-line bg-paper px-3 py-2.5">
              <div className="text-[9.5px] text-muted">预计自动接受</div>
              <div className="mt-1 text-[14px] text-ink">{policyPreview?.length ?? 0} 条</div>
            </div>
          </div>
          <div className="rounded-lg border border-amber/20 bg-amber-soft/30 px-3 py-2.5 text-[10.5px] leading-5 text-amber">
            {policy?.mode === "draft"
              ? "草稿策略不会写入权威库，只报告硬冲突。"
              : policy?.mode === "strict"
                ? "严格策略不会自动通过任何候选。"
                : `L2 还有 ${queue?.counts.l2 ?? 0} 条、L3 还有 ${queue?.counts.l3 ?? 0} 条会留在队列中。`}
          </div>
        </div>
      </Modal>
    </div>
  );
}

function RiskDot({ risk }: { risk: string }) {
  const color =
    risk === "L3" ? "bg-coral" : risk === "L2" ? "bg-amber" : risk === "L1" ? "bg-teal" : "bg-muted";
  return <span className={`h-1.5 w-1.5 shrink-0 rounded-full ${color}`} />;
}

function Info({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-lg border border-line bg-paper px-3 py-2">
      <div className="text-[9px] tracking-wide text-muted">{label}</div>
      <div className="mt-1 break-all font-mono text-[10px] leading-5 text-ink-soft">{value}</div>
    </div>
  );
}

function labelOf(kind: string) {
  return (
    {
      entity: "实体",
      claim: "事实",
      event: "事件",
      relation: "关系",
      promise: "伏笔",
      rule: "规则",
      style_sample: "文笔样本",
      scene_goal: "章节目标",
      knowledge_entry: "知识条目",
      outline_patch: "骨架补丁",
    }[kind] ?? kind
  );
}

function policyLabel(mode: string) {
  return (
    {
      strict: "严格",
      balanced: "平衡",
      fast: "快速",
      draft: "草稿",
    }[mode] ?? mode
  );
}
