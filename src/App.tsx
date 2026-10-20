import { useEffect, useState } from "react";
import { Feather, GitBranch, Library } from "lucide-react";
import { AppShell } from "@/components/AppShell";
import { AuditPage } from "@/features/audit/AuditPage";
import { CanonPage } from "@/features/canon/CanonPage";
import { CharactersPage } from "@/features/characters/CharactersPage";
import { Onboarding } from "@/features/onboarding/Onboarding";
import { PromisesPage } from "@/features/promises/PromisesPage";
import { OverviewPage } from "@/features/project/OverviewPage";
import { PlaceholderPage } from "@/features/placeholder/PlaceholderPage";
import { SeedPage } from "@/features/seed/SeedPage";
import { ipc } from "@/lib/ipc";
import { useAppStore } from "@/stores/ui";

export default function App() {
  const { ready, project, model, nav, toast } = useAppStore();
  const setProject = useAppStore((s) => s.setProject);
  const setModel = useAppStore((s) => s.setModel);
  const setNav = useAppStore((s) => s.setNav);
  const [bootError, setBootError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const [config, snapshot] = await Promise.all([ipc.modelGet(), ipc.projectState()]);
        if (cancelled) return;
        setModel(config);
        setProject(snapshot);
      } catch (e) {
        if (!cancelled) setBootError(e instanceof Error ? e.message : String(e));
      } finally {
        if (!cancelled) useAppStore.setState({ ready: true });
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [setModel, setProject]);

  if (!ready) {
    return (
      <div className="flex h-full items-center justify-center text-[12px] tracking-widest text-muted">
        CANONWEAVE
      </div>
    );
  }

  if (!project) {
    return (
      <>
        <Onboarding model={model} onModel={setModel} onProject={setProject} />
        {bootError ? (
          <div className="fixed bottom-4 right-4 rounded-lg border border-coral/30 bg-paper-raised px-4 py-2 text-[11px] text-coral shadow-lg">
            {bootError}
          </div>
        ) : null}
      </>
    );
  }

  const closeProject = async () => {
    await ipc.closeProject();
    setProject(null);
    setNav("overview");
  };

  return (
    <AppShell project={project} onClose={closeProject}>
      {nav === "overview" ? <OverviewPage project={project} /> : null}
      {nav === "seed" ? <SeedPage /> : null}
      {nav === "canon" ? <CanonPage model={model} /> : null}
      {nav === "characters" ? <CharactersPage /> : null}
      {nav === "timeline" ? (
        <PlaceholderPage
          eyebrow="Map"
          title="双时间线"
          icon={<GitBranch size={20} />}
          emptyTitle="时间线等待第一件事"
          description="故事时间与叙述时间分列呈现，事实的有效区间、分支与关系网络会在这里展开。"
        />
      ) : null}
      {nav === "promises" ? <PromisesPage /> : null}
      {nav === "wiki" ? (
        <PlaceholderPage
          eyebrow="Narrative Wiki"
          title="叙事百科"
          icon={<Library size={20} />}
          emptyTitle="派生视图尚未编译"
          description="百科由已确认事实生成，每页带来源与内容哈希。事实变化后会标记过期并可全量重建。"
        />
      ) : null}
      {nav === "studio" ? (
        <PlaceholderPage
          eyebrow="Studio"
          title="正文工坊"
          icon={<Feather size={20} />}
          emptyTitle="先确认一个章节目标"
          description="场景契约、正文编辑与证据检查器会在这里并列展开。生成草稿先进入候选区，不直接覆盖正文。"
        />
      ) : null}
      {nav === "audit" ? <AuditPage /> : null}
      {toast ? (
        <div className="fixed bottom-5 left-1/2 -translate-x-1/2 rounded-full border border-line bg-paper-raised px-4 py-2 text-[11px] text-ink shadow-lg">
          {toast}
        </div>
      ) : null}
    </AppShell>
  );
}
