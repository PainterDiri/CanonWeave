import { lazy, Suspense, useEffect, useRef, useState } from "react";
import { AppShell } from "@/components/AppShell";
import { AuditPage } from "@/features/audit/AuditPage";
import { CanonPage } from "@/features/canon/CanonPage";
import { CharactersPage } from "@/features/characters/CharactersPage";
import { Onboarding } from "@/features/onboarding/Onboarding";
import { PromisesPage } from "@/features/promises/PromisesPage";
import { OverviewPage } from "@/features/project/OverviewPage";
import { ipc } from "@/lib/ipc";
import { useAppStore } from "@/stores/ui";

const SeedPage = lazy(() =>
  import("@/features/seed/SeedPage").then((module) => ({ default: module.SeedPage })),
);
const StudioPage = lazy(() =>
  import("@/features/studio/StudioPage").then((module) => ({ default: module.StudioPage })),
);
const MapPage = lazy(() =>
  import("@/features/map/MapPage").then((module) => ({ default: module.MapPage })),
);
const AgentPage = lazy(() =>
  import("@/features/agent/AgentPage").then((module) => ({ default: module.AgentPage })),
);
const KnowledgePage = lazy(() =>
  import("@/features/knowledge/KnowledgePage").then((module) => ({
    default: module.KnowledgePage,
  })),
);
const WikiPageView = lazy(() =>
  import("@/features/wiki/WikiPage").then((module) => ({ default: module.WikiPageView })),
);

export default function App() {
  const { ready, project, model, nav, toast } = useAppStore();
  const setProject = useAppStore((s) => s.setProject);
  const setModel = useAppStore((s) => s.setModel);
  const setNav = useAppStore((s) => s.setNav);
  const notify = useAppStore((s) => s.notify);
  const [bootError, setBootError] = useState<string | null>(null);
  const autoIndexedProject = useRef<string | null>(null);

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

  useEffect(() => {
    if (!toast) return;
    const timer = window.setTimeout(() => notify(null), 2800);
    return () => window.clearTimeout(timer);
  }, [toast, notify]);

  useEffect(() => {
    if (!project) {
      autoIndexedProject.current = null;
      return;
    }
    if (
      !model?.embeddingAutoIndex ||
      model.embeddingMode === "off" ||
      autoIndexedProject.current === project.project.id
    ) {
      return;
    }
    autoIndexedProject.current = project.project.id;
    const timer = window.setTimeout(() => {
      void ipc.embeddingIndex(false).catch(() => {
        // Automatic indexing is best-effort; retrieval falls back to FTS + PPR.
      });
    }, 1200);
    return () => window.clearTimeout(timer);
  }, [model?.embeddingAutoIndex, model?.embeddingMode, project]);

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
      {nav === "studio" ? (
        <Suspense
          fallback={
            <div className="flex h-64 items-center justify-center text-[12px] text-muted">
              正在打开正文…
            </div>
          }
        >
          <StudioPage />
        </Suspense>
      ) : null}
      {nav === "agent" ? (
        <Suspense
          fallback={
            <div className="flex h-64 items-center justify-center text-[12px] text-muted">
              正在打开 Agent…
            </div>
          }
        >
          <AgentPage />
        </Suspense>
      ) : null}
      {nav === "seed" ? (
        <Suspense
          fallback={
            <div className="flex h-64 items-center justify-center text-[12px] text-muted">
              正在读取卷章规划…
            </div>
          }
        >
          <SeedPage />
        </Suspense>
      ) : null}
      {nav === "canon" ? <CanonPage model={model} /> : null}
      {nav === "characters" ? <CharactersPage /> : null}
      {nav === "knowledge" ? (
        <Suspense
          fallback={
            <div className="flex h-64 items-center justify-center text-[12px] text-muted">
              正在读取知识库…
            </div>
          }
        >
          <KnowledgePage />
        </Suspense>
      ) : null}
      {nav === "timeline" ? (
        <Suspense
          fallback={
            <div className="flex h-64 items-center justify-center text-[12px] text-muted">
              正在加载叙事图…
            </div>
          }
        >
          <MapPage />
        </Suspense>
      ) : null}
      {nav === "promises" ? <PromisesPage /> : null}
      {nav === "wiki" ? (
        <Suspense
          fallback={
            <div className="flex h-64 items-center justify-center text-[12px] text-muted">
              正在编译叙事百科…
            </div>
          }
        >
          <WikiPageView />
        </Suspense>
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
