import { useEffect, useState } from "react";
import {
  BookOpen,
  LoaderCircle,
  RefreshCw,
  ShieldCheck,
  Sparkles,
  Workflow,
} from "lucide-react";
import { Badge, Button, SectionTitle } from "@/components/ui";
import { ChatWorkspace } from "@/features/seed/ChatWorkspace";
import { ipc, type Premise } from "@/lib/ipc";
import { useAppStore } from "@/stores/ui";

type TaskResult = {
  label: string;
  detail: string;
  target?: "audit" | "timeline" | "wiki" | "seed";
};

export function AgentPage() {
  const project = useAppStore((state) => state.project);
  const setNav = useAppStore((state) => state.setNav);
  const notify = useAppStore((state) => state.notify);
  const refreshProject = useAppStore((state) => state.refreshProject);
  const [premise, setPremise] = useState<Premise | null>(null);
  const [task, setTask] = useState<string | null>(null);
  const [result, setResult] = useState<TaskResult | null>(null);

  useEffect(() => {
    void ipc.premiseActive().then(setPremise).catch(() => setPremise(null));
  }, []);

  const runAudit = async () => {
    setTask("audit");
    try {
      const audit = await ipc.auditRun();
      const issues = await ipc.issues("open");
      setResult({
        label: "一致性巡检完成",
        detail: `本轮新增 ${audit.issuesCreated} 个问题，当前共有 ${issues.length} 项待处理。`,
        target: "audit",
      });
      await refreshProject();
      notify("一致性巡检完成");
    } catch (cause) {
      notify(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setTask(null);
    }
  };

  const rebuildGraph = async () => {
    setTask("graph");
    try {
      const reports = await ipc.communityBuild();
      setResult({
        label: "叙事图谱已整理",
        detail: `已重建 ${reports.length} 个社区摘要，可继续在时间线中检查关系与因果。`,
        target: "timeline",
      });
      notify("叙事图社区摘要已重建");
    } catch (cause) {
      notify(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setTask(null);
    }
  };

  const rebuildWiki = async () => {
    setTask("wiki");
    try {
      const wiki = await ipc.wikiRebuild();
      setResult({
        label: "Narrative Wiki 已重建",
        detail: `已生成 ${wiki.pages.length} 页，包含人物、地点、规则、时间线、伏笔、知识与章节摘要。`,
        target: "wiki",
      });
      notify("Narrative Wiki 已重建");
    } catch (cause) {
      notify(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setTask(null);
    }
  };

  return (
    <div className="fade-in mx-auto max-w-[1340px] px-6 py-6">
      <SectionTitle
        title="协作助手"
        action={
          <div className="flex items-center gap-2">
            <Badge tone="teal">任务编排</Badge>
            <Badge tone="neutral">写入需作者确认</Badge>
          </div>
        }
      />
      <section className="mb-4 flex flex-wrap items-center gap-2 rounded-xl border border-line bg-paper-raised px-3 py-2">
        <span className="mr-1 flex items-center gap-1.5 text-[10.5px] font-medium text-ink-soft">
          <Sparkles size={13} className="text-teal" />
          直接调用现有能力
        </span>
        <Button
          size="sm"
          tone="ghost"
          onClick={() => void runAudit()}
          disabled={Boolean(task)}
          icon={
            task === "audit" ? (
              <LoaderCircle size={12} className="animate-spin" />
            ) : (
              <ShieldCheck size={12} />
            )
          }
        >
          一致性巡检
        </Button>
        <Button
          size="sm"
          tone="ghost"
          onClick={() => void rebuildGraph()}
          disabled={Boolean(task)}
          icon={
            task === "graph" ? (
              <LoaderCircle size={12} className="animate-spin" />
            ) : (
              <Workflow size={12} />
            )
          }
        >
          重建叙事图
        </Button>
        <Button
          size="sm"
          tone="ghost"
          onClick={() => void rebuildWiki()}
          disabled={Boolean(task)}
          icon={
            task === "wiki" ? (
              <LoaderCircle size={12} className="animate-spin" />
            ) : (
              <RefreshCw size={12} />
            )
          }
        >
          重建知识视图
        </Button>
        <Button
          size="sm"
          tone="ghost"
          onClick={() => setNav("seed")}
          icon={<BookOpen size={12} />}
        >
          规划卷章
        </Button>
        <span className="ml-auto text-[9.5px] text-muted">
          {project ? `${project.counts.openIssues} 个冲突 · ${project.counts.pendingCandidates} 个候选` : ""}
        </span>
      </section>

      {result ? (
        <div className="mb-4 flex items-center justify-between gap-4 rounded-xl border border-teal/20 bg-teal-soft/35 px-4 py-3">
          <div>
            <div className="text-[11.5px] font-medium text-teal">{result.label}</div>
            <div className="mt-0.5 text-[10.5px] leading-5 text-ink-soft">{result.detail}</div>
          </div>
          {result.target ? (
            <Button size="sm" onClick={() => setNav(result.target!)}>
              查看结果
            </Button>
          ) : null}
        </div>
      ) : null}

      <ChatWorkspace premise={premise} surface="agent" />
    </div>
  );
}
