import { useEffect, useState } from "react";
import { Bot, ChevronRight } from "lucide-react";
import { ipc, type AgentRun } from "@/lib/ipc";
import { useAppStore } from "@/stores/ui";

export function AgentSuggestion({ outputs }: { outputs?: string[] }) {
  const project = useAppStore((state) => state.project);
  const setNav = useAppStore((state) => state.setNav);
  const [run, setRun] = useState<AgentRun | null>(null);
  const outputKey = outputs?.join(",") ?? "";

  useEffect(() => {
    if (!project) return;
    void ipc
      .agentRuns(12)
      .then((runs) => {
        setRun(
          runs.find(
            (item) =>
              (!outputs || outputs.includes(item.output)) &&
              ["waiting_approval", "failed", "paused"].includes(item.status),
          ) ?? null,
        );
      })
      .catch(() => setRun(null));
  }, [outputKey, project]);

  if (!run) return null;
  const message =
    run.status === "waiting_approval"
      ? `${run.pendingCandidates} 项候选等待核对`
      : run.status === "failed"
        ? "最近一次任务失败，可从失败步骤重试"
        : "最近一次任务已安全暂停，可从检查点恢复";

  return (
    <button
      onClick={() => setNav("agent")}
      className="focus-ring mb-4 flex w-full items-center gap-3 rounded-xl border border-amber/20 bg-amber-soft/25 px-3.5 py-2.5 text-left transition hover:border-amber/35 hover:bg-amber-soft/40"
    >
      <span className="flex h-7 w-7 shrink-0 items-center justify-center rounded-lg bg-paper-raised text-amber">
        <Bot size={13} />
      </span>
      <span className="min-w-0 flex-1">
        <span className="block text-[10px] font-semibold text-ink-soft">Agent 最近建议</span>
        <span className="mt-0.5 block truncate text-[9.5px] text-muted">
          {message} · {run.goal}
        </span>
      </span>
      <ChevronRight size={12} className="shrink-0 text-amber" />
    </button>
  );
}
