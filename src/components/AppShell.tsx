import { useEffect, useState, type ReactNode } from "react";
import {
  BookOpen,
  Bot,
  Boxes,
  Compass,
  Database,
  Feather,
  GitBranch,
  Library,
  Map as MapIcon,
  ShieldCheck,
  Sparkles,
  Waypoints,
} from "lucide-react";
import type { ProjectSnapshot } from "@/lib/ipc";
import { ipc, type BranchRecord } from "@/lib/ipc";
import { useAppStore, type NavKey, type WorkspaceMode } from "@/stores/ui";

type NavItem = {
  key: NavKey;
  label: string;
  icon: typeof Compass;
  mode: WorkspaceMode;
};

const navGroups: { label: string | null; items: NavItem[] }[] = [
  {
    label: null,
    items: [{ key: "overview", label: "概览", icon: Compass, mode: "studio" }],
  },
  {
    label: "写作",
    items: [
      { key: "studio", label: "正文", icon: BookOpen, mode: "studio" },
      { key: "agent", label: "协作助手", icon: Bot, mode: "studio" },
    ],
  },
  {
    label: "故事资料",
    items: [
      { key: "characters", label: "人物", icon: Waypoints, mode: "map" },
      { key: "knowledge", label: "知识库", icon: Database, mode: "map" },
      { key: "timeline", label: "时间线", icon: GitBranch, mode: "map" },
      { key: "promises", label: "伏笔", icon: Feather, mode: "audit" },
      { key: "wiki", label: "叙事百科", icon: Library, mode: "map" },
    ],
  },
  {
    label: "质量",
    items: [{ key: "audit", label: "一致性审计", icon: ShieldCheck, mode: "audit" }],
  },
  {
    label: "项目",
    items: [
      { key: "seed", label: "灵感与卷章", icon: Sparkles, mode: "seed" },
      { key: "canon", label: "设定与配置", icon: Boxes, mode: "seed" },
    ],
  },
];

export function AppShell({
  project,
  children,
  onClose,
}: {
  project: ProjectSnapshot;
  children: ReactNode;
  onClose: () => void;
}) {
  const nav = useAppStore((s) => s.nav);
  const setNav = useAppStore((s) => s.setNav);
  const setMode = useAppStore((s) => s.setMode);
  const refreshProject = useAppStore((s) => s.refreshProject);
  const notify = useAppStore((s) => s.notify);
  const [branches, setBranches] = useState<BranchRecord[]>([]);
  const navItems = navGroups.flatMap((group) => group.items);

  useEffect(() => {
    void ipc
      .branches()
      .then(setBranches)
      .catch(() => {});
  }, []);

  const switchBranch = async (id: string) => {
    try {
      setBranches(await ipc.branchSwitch(id));
      await refreshProject();
    } catch {
      notify("切换分支失败");
    }
  };

  const addBranch = async () => {
    const name = window.prompt("新分支名称");
    if (!name?.trim()) return;
    try {
      await ipc.branchCreate(name.trim());
      setBranches(await ipc.branches());
      await refreshProject();
      notify(`已建立分支：${name}`);
    } catch {
      notify("建立分支失败");
    }
  };

  const go = (key: NavKey) => {
    const item = navItems.find((n) => n.key === key);
    if (item) setMode(item.mode);
    setNav(key);
  };

  const activeLabel = navItems.find((item) => item.key === nav)?.label ?? "概览";

  return (
    <div className="flex h-full min-h-0">
      <aside className="flex w-[216px] shrink-0 flex-col border-r border-line bg-paper/60">
        <div className="window-drag flex h-[54px] items-center pl-[76px] pr-4">
          <div className="flex items-center gap-2.5">
            <div className="flex h-7 w-7 items-center justify-center rounded-lg bg-teal">
              <svg width="17" height="17" viewBox="0 0 24 24" fill="none">
                <path d="M7 16.5c0-2.5 2-4.5 4.5-4.5s4.5 2 4.5 4.5" stroke="#F3EFE6" strokeWidth="1.8" strokeLinecap="round" />
                <path d="M17 7.5c0 2.5-2 4.5-4.5 4.5S8 10 8 7.5" stroke="#D9A441" strokeWidth="1.5" strokeLinecap="round" />
                <circle cx="12" cy="12" r="1.6" fill="#E2725B" />
              </svg>
            </div>
            <div>
              <div className="text-[13px] font-semibold tracking-wide leading-none text-ink">CanonWeave</div>
              <div className="mt-1 text-[9px] tracking-[0.22em] text-muted">叙事织网</div>
            </div>
          </div>
        </div>

        <nav className="mt-2 flex-1 overflow-y-auto px-3 pb-2">
          {navGroups.map((group, groupIndex) => (
            <div key={group.label ?? `group-${groupIndex}`} className={groupIndex ? "mt-4" : ""}>
              {group.label ? (
                <div className="mb-1.5 px-3 text-[9px] font-semibold tracking-[0.16em] text-muted/75">
                  {group.label}
                </div>
              ) : null}
              <div className="space-y-0.5">
                {group.items.map((item) => {
                  const Icon = item.icon;
                  const active = nav === item.key;
                  return (
                    <button
                      key={item.key}
                      onClick={() => go(item.key)}
                      className={`focus-ring group flex h-9 w-full items-center gap-2.5 rounded-lg px-3 text-left transition ${
                        active
                          ? "bg-teal text-paper-raised"
                          : "text-ink-soft hover:bg-ink/5"
                      }`}
                    >
                      <Icon
                        size={15}
                        strokeWidth={active ? 2 : 1.7}
                        className={active ? "text-paper-raised" : "text-muted"}
                      />
                      <span className="text-[12px] font-medium">{item.label}</span>
                      {item.key === "audit" && project.counts.openIssues > 0 ? (
                        <span className="ml-auto flex h-4 min-w-4 items-center justify-center rounded-full bg-coral px-1 text-[9px] font-semibold text-white">
                          {project.counts.openIssues}
                        </span>
                      ) : null}
                      {item.key === "agent" && project.counts.pendingCandidates > 0 ? (
                        <span className="ml-auto rounded-full bg-amber-soft px-1.5 text-[9px] font-medium text-amber">
                          {project.counts.pendingCandidates}
                        </span>
                      ) : null}
                    </button>
                  );
                })}
              </div>
            </div>
          ))}
        </nav>

        <div className="border-t border-line px-3 py-3">
          <button
            onClick={onClose}
            className="group w-full rounded-lg px-3 py-2 text-left transition hover:bg-ink/5"
            title="关闭项目"
          >
            <div className="truncate text-[11.5px] font-medium text-ink-soft">{project.project.name}</div>
            <div className="mt-1 flex items-center gap-1.5 text-[10px] text-muted">
              <span className="h-1 w-1 rounded-full bg-teal" />
              本地项目 · 已保存
            </div>
          </button>
        </div>
      </aside>

      <div className="flex min-w-0 flex-1 flex-col">
        <header className="window-drag flex h-[54px] shrink-0 items-center justify-between border-b border-line bg-paper/50 px-5">
          <div className="no-drag flex items-center gap-4">
            <div className="flex items-center gap-2 text-[11px]">
              <span className="font-medium text-ink">{project.project.name}</span>
              <span className="text-muted">/</span>
              <span className="text-ink-soft">{activeLabel}</span>
            </div>
            <div className="hidden h-4 w-px bg-line md:block" />
            <div className="hidden items-center gap-1.5 text-[10.5px] text-muted lg:flex">
              <GitBranch size={12} className="text-teal/60" />
              <select
                value={branches.find((branch) => branch.isCurrent)?.id ?? project.project.activeBranchId ?? ""}
                onChange={(e) => void switchBranch(e.target.value)}
                className="max-w-[110px] rounded border border-line bg-paper-raised px-1.5 py-0.5 text-[10px] text-ink-soft"
              >
                {branches.map((branch) => (
                  <option key={branch.id} value={branch.id}>
                    {branch.name}
                  </option>
                ))}
              </select>
              <button
                onClick={() => void addBranch()}
                className="rounded px-1 text-teal/70 hover:bg-teal-soft"
                title="从当前分支建立新分支"
              >
                +
              </button>
            </div>
            <HeaderMeta icon={<MapIcon size={12} />} label="POV 未设定" />
            <HeaderMeta icon={<Compass size={12} />} label="时间未设定" />
          </div>
          <div className="no-drag flex items-center gap-3 text-[10.5px] text-muted">
            <span className="flex items-center gap-1.5">
              <span className="h-1.5 w-1.5 rounded-full bg-teal/70" />
              已保存
            </span>
            <span className="h-3 w-px bg-line" />
            <span>空闲</span>
          </div>
        </header>

        <main className="min-h-0 flex-1 overflow-y-auto">{children}</main>
      </div>
    </div>
  );
}

function HeaderMeta({ icon, label }: { icon: ReactNode; label: string }) {
  return (
    <div className="hidden items-center gap-1.5 text-[10.5px] text-muted lg:flex">
      <span className="text-teal/60">{icon}</span>
      {label}
    </div>
  );
}
