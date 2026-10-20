import {
  BookOpen,
  Boxes,
  Compass,
  Feather,
  Flag,
  Network,
  ShieldCheck,
  Sparkles,
} from "lucide-react";
import { Badge, Button, SectionTitle } from "@/components/ui";
import type { ProjectSnapshot } from "@/lib/ipc";
import { useAppStore } from "@/stores/ui";

export function OverviewPage({ project }: { project: ProjectSnapshot }) {
  const setNav = useAppStore((s) => s.setNav);
  const stats = [
    ["待确认候选", project.counts.pendingCandidates, "项"],
    ["章节", project.counts.chapters, "章"],
    ["人物与实体", project.counts.entities, "个"],
    ["未结伏笔", project.counts.promises, "条"],
  ];
  const checklist = [
    { label: "确认种子卡", done: project.counts.premises > 0, target: "seed" as const },
    { label: "生成十章骨架", done: project.counts.chapters > 0, target: "seed" as const },
    { label: "建立文笔档案", done: false, target: "canon" as const },
    { label: "开始第一场", done: false, target: "studio" as const },
  ];

  return (
    <div className="fade-in mx-auto max-w-[1120px] px-8 py-7">
      <SectionTitle
        eyebrow="Project overview"
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

      <div className="grid grid-cols-4 gap-3">
        {stats.map(([label, value, unit]) => (
          <div key={String(label)} className="paper-card px-5 py-4">
            <div className="text-[11px] tracking-wide text-muted">{label}</div>
            <div className="mt-2 flex items-baseline gap-1">
              <span className="display text-[30px] leading-none text-ink">{value}</span>
              <span className="text-[11px] text-muted">{unit}</span>
            </div>
          </div>
        ))}
      </div>

      <div className="mt-5 grid grid-cols-[1.45fr_1fr] gap-5">
        <section className="paper-card p-6">
          <div className="mb-5 flex items-center justify-between">
            <div>
              <div className="text-[10px] font-semibold uppercase tracking-[0.18em] text-teal">
                Next steps
              </div>
              <h2 className="display mt-1 text-[20px] text-ink">把故事从一句话推向第一场</h2>
            </div>
            <Compass size={18} className="text-teal/60" />
          </div>
          <div className="space-y-2">
            {checklist.map((item, index) => (
              <button
                key={item.label}
                onClick={() => setNav(item.target)}
                className="group flex w-full items-center gap-3 rounded-xl border border-transparent px-3 py-2.5 text-left transition hover:border-line hover:bg-paper"
              >
                <div
                  className={`flex h-6 w-6 items-center justify-center rounded-full border text-[10px] ${
                    item.done
                      ? "border-teal/25 bg-teal text-paper-raised"
                      : "border-line-strong text-muted"
                  }`}
                >
                  {item.done ? "✓" : index + 1}
                </div>
                <div className="flex-1">
                  <div className={`text-[13px] ${item.done ? "text-muted line-through" : "text-ink"}`}>
                    {item.label}
                  </div>
                </div>
                <span className="text-[11px] text-muted opacity-0 transition group-hover:opacity-100">
                  前往
                </span>
              </button>
            ))}
          </div>
          <div className="hairline my-5" />
          <div className="flex items-center gap-3">
            <Button tone="primary" onClick={() => setNav("seed")} icon={<Sparkles size={14} />}>
              从灵感开始
            </Button>
            <Button onClick={() => setNav("studio")} icon={<Feather size={14} />}>
              进入正文
            </Button>
          </div>
        </section>

        <div className="space-y-5">
          <section className="paper-card p-5">
            <div className="mb-4 flex items-center gap-2 text-[13px] font-semibold text-ink">
              <ShieldCheck size={15} className="text-teal" />
              一致性状态
            </div>
            <div className="space-y-3 text-[12px]">
              <StatusRow label="硬冲突" value={`${project.counts.openIssues} 项待处理`} ok={project.counts.openIssues === 0} />
              <StatusRow label="候选事实" value={`${project.counts.pendingCandidates} 项待审核`} ok={project.counts.pendingCandidates === 0} />
              <StatusRow label="派生视图" value="尚未建立" ok={false} neutral />
            </div>
          </section>

          <section className="paper-card p-5">
            <div className="mb-3 flex items-center gap-2 text-[13px] font-semibold text-ink">
              <Boxes size={15} className="text-teal" />
              项目位置
            </div>
            <div className="rounded-lg border border-line bg-paper px-3 py-2.5 font-mono text-[10.5px] leading-5 text-muted break-all">
              {project.project.rootPath}
            </div>
            <div className="mt-3 flex items-center gap-2 text-[11px] text-muted">
              <span className="h-1 w-1 rounded-full bg-teal" />
              数据只保存在本地 · schema v{project.project.schemaVersion}
            </div>
          </section>
        </div>
      </div>

      <section className="paper-card mt-5 p-6">
        <div className="mb-4 flex items-center justify-between">
          <div className="flex items-center gap-2 text-[13px] font-semibold text-ink">
            <Network size={15} className="text-teal" />
            故事织网
          </div>
          <span className="text-[11px] text-muted">随写作逐步建立</span>
        </div>
        <div className="grid grid-cols-4 gap-3">
          {[
            { icon: Compass, label: "种子与骨架", nav: "seed" as const },
            { icon: BookOpen, label: "人物与设定", nav: "characters" as const },
            { icon: Flag, label: "伏笔台账", nav: "promises" as const },
            { icon: ShieldCheck, label: "冲突审计", nav: "audit" as const },
          ].map((item) => {
            const Icon = item.icon;
            return (
              <button
                key={item.label}
                onClick={() => setNav(item.nav)}
                className="group flex items-center gap-3 rounded-xl border border-line bg-paper px-4 py-3 text-left transition hover:border-teal/40 hover:bg-paper-raised"
              >
                <Icon size={16} className="text-teal/70" />
                <span className="text-[12px] text-ink-soft group-hover:text-ink">{item.label}</span>
              </button>
            );
          })}
        </div>
      </section>
    </div>
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
      <span className={neutral ? "text-muted" : ok ? "text-teal" : "text-amber"}>{value}</span>
    </div>
  );
}
