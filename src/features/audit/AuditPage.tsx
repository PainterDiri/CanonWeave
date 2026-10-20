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
import { Badge, Button, SectionTitle } from "@/components/ui";
import { summaryOf } from "@/features/seed/ChatWorkspace";
import { ipc, type ReviewQueue } from "@/lib/ipc";
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
];

export function AuditPage() {
  const [queue, setQueue] = useState<ReviewQueue | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const [kind, setKind] = useState("");
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const refreshProject = useAppStore((s) => s.refreshProject);

  const load = useCallback(async () => {
    try {
      const result = await ipc.reviewQueue();
      setQueue(result);
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
    <div className="fade-in mx-auto max-w-[1180px] px-8 py-7">
      <SectionTitle
        eyebrow="Audit"
        title="一致性审计"
        action={
          <div className="flex items-center gap-2">
            <Badge tone="teal">平衡策略</Badge>
            <Badge tone="neutral">L0 自动通过：关闭</Badge>
          </div>
        }
      />

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
        <div className="grid grid-cols-[340px_minmax(0,1fr)] gap-4">
          <section className="paper-card flex h-[560px] min-h-0 flex-col overflow-hidden">
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
            <section className="paper-card flex h-[560px] min-h-0 flex-col overflow-hidden">
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
    }[kind] ?? kind
  );
}
