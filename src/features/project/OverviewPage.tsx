import { useEffect, useState } from "react";
import { ArrowRight, Bot, Check, Feather, LoaderCircle, ShieldCheck } from "lucide-react";
import { Badge, Button, SectionTitle } from "@/components/ui";
import {
  ipc,
  type CraftReport,
  type PipelineStage,
  type ProjectSnapshot,
  type WorkflowPipeline,
} from "@/lib/ipc";
import { useAppStore, type NavKey } from "@/stores/ui";

const navKeys: NavKey[] = [
  "overview",
  "studio",
  "agent",
  "seed",
  "canon",
  "characters",
  "knowledge",
  "timeline",
  "promises",
  "wiki",
  "audit",
];

function toNav(value: string): NavKey {
  return (navKeys as string[]).includes(value) ? (value as NavKey) : "overview";
}

export function OverviewPage({ project }: { project: ProjectSnapshot }) {
  const setNav = useAppStore((s) => s.setNav);
  const setAgentDraft = useAppStore((s) => s.setAgentDraft);
  const [pipeline, setPipeline] = useState<WorkflowPipeline | null>(null);
  const [craft, setCraft] = useState<CraftReport | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let alive = true;
    void Promise.all([
      ipc.workflowPipeline().catch(() => null),
      ipc.craftReport().catch(() => null),
    ]).then(([nextPipeline, nextCraft]) => {
      if (!alive) return;
      setPipeline(nextPipeline);
      setCraft(nextCraft);
      setLoading(false);
    });
    return () => {
      alive = false;
    };
  }, [project.counts]);

  const stages = pipeline?.stages ?? [];
  const required = stages.filter((stage) => stage.status !== "optional");
  const doneCount = required.filter((stage) => stage.status === "done").length;
  const active = stages.find((stage) => stage.key === pipeline?.activeKey || stage.status === "active");
  const donePercent = required.length ? Math.round((doneCount / required.length) * 100) : 0;

  const open = (stage: PipelineStage) => {
    if (stage.key === "guide") window.location.hash = "guide";
    setNav(toNav(stage.nav));
  };

  const delegate = (stage: PipelineStage) => {
    setAgentDraft(stage.prompt);
    setNav("agent");
  };

  return (
    <div className="fade-in mx-auto max-w-[1120px] px-8 py-7">
      <SectionTitle
        title={project.project.name}
        action={
          <div className="flex items-center gap-2">
            <Badge tone="teal">
              {project.project.mode === "idea"
                ? "一句话起步"
                : project.project.mode === "import"
                  ? "旧稿导入"
                  : "混合模式"}
            </Badge>
            <Button tone="ghost" size="sm" onClick={() => setNav("canon")}>
              项目设置
            </Button>
          </div>
        }
      />

      <div className="grid grid-cols-[minmax(0,1.55fr)_minmax(280px,1fr)] gap-5">
        <section className="paper-card px-6 pb-5 pt-6">
          <div className="mb-1 flex items-end justify-between gap-4">
            <div>
              <h2 className="display text-[21px] text-ink">创作流水线</h2>
              <p className="mt-1 text-[12px] leading-6 text-muted">
                先立契约与故事圣经，再定因果、卷章和每章的价值转变，最后起草与修订。阶段由项目数据自动推导。
              </p>
            </div>
            <div className="shrink-0 text-right">
              <div className="display text-[26px] leading-none text-teal">
                {doneCount}
                <span className="text-[14px] text-muted"> / {required.length || 7}</span>
              </div>
              <div className="mt-1 text-[10.5px] text-muted">必经阶段完成</div>
            </div>
          </div>

          {loading ? (
            <div className="flex h-56 items-center justify-center text-[12px] text-muted">
              <LoaderCircle size={14} className="mr-2 animate-spin" /> 正在推导流水线
            </div>
          ) : stages.length === 0 ? (
            <div className="py-10 text-center text-[12px] text-muted">流水线暂不可用，请稍后刷新。</div>
          ) : (
            <ol
              className="relative mt-5"
              style={{ ["--done" as string]: `${donePercent}%` }}
              aria-label="创作流水线阶段"
            >
              <div className="pipeline-thread" aria-hidden />
              {stages.map((stage) => (
                <StageRow
                  key={stage.key}
                  stage={stage}
                  onOpen={() => open(stage)}
                  onDelegate={() => delegate(stage)}
                />
              ))}
            </ol>
          )}
        </section>

        <div className="space-y-5">
          <section className="paper-card p-5">
            <h2 className="mb-1 text-[13px] font-semibold text-ink">现在最该做的</h2>
            {active ? (
              <>
                <div className="display mt-2 text-[19px] text-teal-deep">{active.label}</div>
                <p className="mt-1.5 text-[12px] leading-6 text-ink-soft">{active.description}</p>
                <p className="mt-1 text-[11px] text-muted">{active.detail}</p>
                <div className="mt-4 flex flex-wrap gap-2">
                  <Button tone="primary" size="sm" icon={<Bot size={13} />} onClick={() => delegate(active)}>
                    交给 Agent
                  </Button>
                  <Button size="sm" icon={<ArrowRight size={13} />} onClick={() => open(active)}>
                    自己动手
                  </Button>
                </div>
              </>
            ) : (
              <p className="mt-2 text-[12px] leading-6 text-muted">所有阶段都已完成，继续写作与修订即可。</p>
            )}
          </section>

          <section className="paper-card p-5">
            <div className="mb-3 flex items-center gap-2 text-[13px] font-semibold text-ink">
              <ShieldCheck size={15} className="text-teal" />
              一致性与工艺
            </div>
            <div className="space-y-2.5 text-[12px]">
              <StatusRow label="未处理冲突" value={`${project.counts.openIssues} 项`} ok={project.counts.openIssues === 0} />
              <StatusRow
                label="待确认候选"
                value={`${project.counts.pendingCandidates} 项`}
                ok={project.counts.pendingCandidates === 0}
              />
              <StatusRow
                label="章节价值转变"
                value={craft ? `${craft.chaptersWithShift}/${craft.chapterCount} 章` : "—"}
                ok={Boolean(craft && craft.chapterCount > 0 && craft.chaptersWithShift === craft.chapterCount)}
              />
              <StatusRow
                label="未关闭叙事线程"
                value={craft ? `${craft.openThreads} 条` : "—"}
                ok={Boolean(craft && craft.openThreads === 0)}
                neutral
              />
            </div>
            {craft && craft.checks.length > 0 ? (
              <ul className="mt-4 space-y-1.5 border-t border-line pt-3">
                {craft.checks.slice(0, 3).map((check, index) => (
                  <li key={`${check.code}-${index}`} className="text-[11px] leading-5 text-ink-soft">
                    <span className={check.severity === "warning" ? "text-amber" : "text-muted"}>
                      {check.severity === "warning" ? "需留意" : "提示"}
                    </span>{" "}
                    {check.message}
                  </li>
                ))}
              </ul>
            ) : null}
          </section>

          <section className="paper-card p-5">
            <h2 className="mb-3 text-[13px] font-semibold text-ink">项目位置</h2>
            <div className="rounded-lg border border-line bg-paper px-3 py-2.5 font-mono text-[10.5px] leading-5 break-all text-muted">
              {project.project.rootPath}
            </div>
            <div className="mt-3 flex items-center justify-between text-[11px] text-muted">
              <span>数据只保存在本地 · schema v{project.project.schemaVersion}</span>
              <button
                onClick={() => setNav("studio")}
                className="flex items-center gap-1 text-teal hover:underline"
              >
                <Feather size={12} /> 进入正文
              </button>
            </div>
          </section>
        </div>
      </div>
    </div>
  );
}

function StageRow({
  stage,
  onOpen,
  onDelegate,
}: {
  stage: PipelineStage;
  onOpen: () => void;
  onDelegate: () => void;
}) {
  const done = stage.status === "done";
  const activeStage = stage.status === "active";
  const optional = stage.status === "optional";
  return (
    <li
      className={`group relative flex gap-4 rounded-xl py-2.5 pl-0 pr-2 transition ${
        activeStage ? "bg-teal-soft/45" : "hover:bg-paper/80"
      }`}
    >
      <span
        className={`relative z-[1] mt-0.5 flex h-8 w-8 shrink-0 items-center justify-center rounded-full border text-[11px] ${
          done
            ? "border-teal bg-teal text-paper-raised"
            : activeStage
              ? "border-teal bg-paper-raised text-teal"
              : "border-silk bg-paper-raised text-muted"
        }`}
      >
        {done ? (
          <Check size={14} />
        ) : (
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" aria-hidden>
            <path
              d="M6 18C5.6 11 9.8 6.2 18 5.6c.4 7.8-4.2 12.2-12 12.4Z"
              fill={activeStage ? "currentColor" : "none"}
              fillOpacity={activeStage ? 0.25 : 0}
              stroke="currentColor"
              strokeWidth="1.6"
            />
          </svg>
        )}
      </span>
      <div className="min-w-0 flex-1">
        <div className="flex items-baseline gap-2">
          <span className={`text-[13.5px] font-medium ${done ? "text-ink-soft" : "text-ink"}`}>{stage.label}</span>
          {optional ? <span className="text-[10px] text-muted">可选</span> : null}
          {activeStage ? <span className="text-[10px] font-medium text-teal">当前</span> : null}
          <span className="ml-auto shrink-0 text-[10.5px] text-muted">{stage.detail}</span>
        </div>
        <p className="mt-0.5 text-[11.5px] leading-5 text-muted">{stage.description}</p>
        {!done ? (
          <div className="mt-1.5 flex items-center gap-3">
            <div className="h-[3px] w-24 overflow-hidden rounded-full bg-silk-soft">
              <div className="h-full rounded-full bg-teal" style={{ width: `${Math.round((stage.progress ?? 0) * 100)}%` }} />
            </div>
            <button
              onClick={onOpen}
              className="text-[10.5px] text-ink-soft opacity-70 transition group-hover:opacity-100 hover:text-teal"
            >
              前往
            </button>
            <button
              onClick={onDelegate}
              className="text-[10.5px] text-teal opacity-70 transition group-hover:opacity-100 hover:underline"
            >
              交给 Agent
            </button>
          </div>
        ) : null}
      </div>
    </li>
  );
}

function StatusRow({
  label,
  value,
  ok,
  neutral,
}: {
  label: string;
  value: string;
  ok: boolean;
  neutral?: boolean;
}) {
  return (
    <div className="flex items-center justify-between">
      <span className="text-muted">{label}</span>
      <span className={neutral ? "text-ink-soft" : ok ? "text-teal" : "text-amber"}>{value}</span>
    </div>
  );
}
