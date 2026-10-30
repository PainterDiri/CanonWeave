import { Markdown } from "@/components/Markdown";
import { useCallback, useEffect, useMemo, useState } from "react";
import {
  BookOpen,
  FileWarning,
  Library,
  LoaderCircle,
  Pencil,
  RefreshCw,
  ShieldCheck,
  Sparkles,
} from "lucide-react";
import {
  Badge,
  Button,
  EmptyState,
  Field,
  Input,
  Modal,
  SectionTitle,
  Textarea,
} from "@/components/ui";
import { ipc, type Candidate, type WikiPage } from "@/lib/ipc";
import { useAppStore } from "@/stores/ui";

const kindMeta: Record<string, { label: string; short: string }> = {
  character: { label: "人物档案", short: "人物" },
  place: { label: "地点档案", short: "地点" },
  world_rule: { label: "世界规则", short: "规则" },
  timeline: { label: "时间线", short: "时间" },
  promise: { label: "伏笔", short: "伏笔" },
  chapter: { label: "章节摘要", short: "章节" },
  knowledge: { label: "知识条目", short: "知识" },
};

export function WikiPageView() {
  const [pages, setPages] = useState<WikiPage[]>([]);
  const [pendingEdits, setPendingEdits] = useState<Candidate[]>([]);
  const [dependencies, setDependencies] = useState<
    Awaited<ReturnType<typeof ipc.wikiDependencies>>
  >([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [kind, setKind] = useState("all");
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [source, setSource] = useState<string | null>(null);
  const [editing, setEditing] = useState<{ title: string; content: string } | null>(null);
  const refreshProject = useAppStore((s) => s.refreshProject);
  const notify = useAppStore((s) => s.notify);

  const load = useCallback(async () => {
    try {
      const [pageList, pending, dependencyList] = await Promise.all([
        ipc.wikiList(),
        ipc.candidateList("pending", "wiki_edit"),
        ipc.wikiDependencies(),
      ]);
      setPages(pageList);
      setPendingEdits(pending);
      setDependencies(dependencyList);
      setSelectedId((current) =>
        current && pageList.some((page) => page.id === current)
          ? current
          : (pageList[0]?.id ?? null),
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

  const rebuild = async () => {
    setBusy(true);
    setError(null);
    try {
      const result = await ipc.wikiRebuild();
      setPages(result.pages);
      setDependencies(await ipc.wikiDependencies());
      setSelectedId(result.pages[0]?.id ?? null);
      notify(
        `百科已重建：${result.characters} 人物 / ${result.places} 地点 / ${result.rules} 规则 / ${result.chapters} 章节 · 保留 ${result.preservedAuthorPages} 个作者页`,
      );
      await refreshProject();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const compileChapters = async () => {
    setBusy(true);
    setError(null);
    try {
      const result = await ipc.wikiCompileLlm(4);
      setPages(result.pages);
      setDependencies(await ipc.wikiDependencies());
      notify(
        `模型编译章节摘要 ${result.compiled} 页；跳过 ${result.skipped} 页，剩余 ${result.remaining} 页`,
      );
      if (result.failed.length) {
        setError(`部分章节未更新：${result.failed.slice(0, 3).join("；")}`);
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const filtered = useMemo(
    () => pages.filter((page) => kind === "all" || page.kind === kind),
    [pages, kind],
  );
  const current = filtered.find((page) => page.id === selectedId) ?? filtered[0] ?? null;
  const pageEdits = pendingEdits.filter(
    (candidate) => candidate.payload.pageId === current?.id,
  );
  const pageDependencies = dependencies.filter(
    (dependency) => dependency.pageId === current?.id,
  );

  const proposeEdit = async () => {
    if (!current || !editing) return;
    setBusy(true);
    try {
      await ipc.wikiProposeEdit({
        pageId: current.id,
        title: editing.title,
        content: editing.content,
      });
      setEditing(null);
      await load();
      notify("已提出百科修改候选；确认后只更新派生页面，不修改正式事实。");
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const confirmEdit = async (candidate: Candidate) => {
    setBusy(true);
    try {
      await ipc.candidateAccept(candidate.id);
      setEditing(null);
      await load();
      notify("百科修改候选已确认；正式事实表未被改写。");
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const filters = [
    ["all", "全部"],
    ...Object.entries(kindMeta).map(([key, value]) => [key, value.short]),
  ];

  return (
    <div className="fade-in workspace-page px-8 py-7">
      <SectionTitle
        eyebrow="Narrative Wiki Compiler"
        title="叙事百科"
        action={
          <div className="flex gap-2">
            <Button
              size="sm"
              onClick={() => void compileChapters()}
              disabled={busy}
              icon={<Sparkles size={12} />}
            >
              AI 编译章节
            </Button>
            <Button
              size="sm"
              tone="primary"
              onClick={() => void rebuild()}
              disabled={busy}
              icon={
                busy ? <LoaderCircle size={12} className="animate-spin" /> : <RefreshCw size={12} />
              }
            >
              重建并合并
            </Button>
          </div>
        }
      />
      <div className="mb-5 flex flex-wrap items-center gap-1.5 border-b border-line pb-3">
        {filters.map(([key, label]) => (
          <button
            key={key}
            onClick={() => {
              setKind(key);
              setSelectedId(null);
            }}
            className={`rounded-full px-3 py-1.5 text-[10.5px] font-medium transition ${
              kind === key ? "bg-teal text-paper-raised" : "bg-paper-raised text-muted hover:text-ink"
            }`}
          >
            {label}
            <span className="ml-1.5 opacity-60">
              {key === "all" ? pages.length : pages.filter((page) => page.kind === key).length}
            </span>
          </button>
        ))}
        <div className="ml-auto text-[9.5px] text-muted">
          派生视图 · 非权威事实 · 事实变化后标记过期
        </div>
      </div>
      {error ? (
        <div className="mb-4 rounded-lg border border-coral/25 bg-coral-soft/60 px-4 py-2.5 text-[11.5px] text-coral">
          {error}
        </div>
      ) : null}
      {loading ? (
        <div className="flex h-64 items-center justify-center text-[12px] text-muted">
          <LoaderCircle size={15} className="mr-2 animate-spin" /> 正在读取百科页面
        </div>
      ) : pages.length === 0 ? (
        <EmptyState
          icon={<Library size={20} />}
          title="派生视图尚未编译"
          description="从已确认人物、地点、规则、事件、伏笔与章节版本生成页面。每页保留来源 ID、原文片段、内容哈希与过期状态。"
          action={
            <Button tone="primary" onClick={() => void rebuild()} disabled={busy}>
              建立第一版百科
            </Button>
          }
        />
      ) : (
        <div className="workspace-fill grid grid-cols-[300px_minmax(0,1fr)] gap-4">
          <aside className="paper-card flex h-full min-h-0 flex-col overflow-hidden">
            <div className="border-b border-line px-4 py-3 text-[11px] font-semibold text-ink-soft">
              编译页面 {filtered.length}
            </div>
            <div className="min-h-0 flex-1 overflow-y-auto p-2">
              {filtered.map((page) => (
                <button
                  key={page.id}
                  onClick={() => setSelectedId(page.id)}
                  className={`mb-1 w-full rounded-lg px-3 py-2.5 text-left transition ${
                    current?.id === page.id ? "bg-teal-soft/70" : "hover:bg-ink/[0.03]"
                  }`}
                >
                  <div className="flex items-center justify-between gap-2">
                    <span className="truncate text-[11.5px] font-medium text-ink">
                      {page.title}
                    </span>
                    {page.staleAt ? <FileWarning size={12} className="shrink-0 text-amber" /> : null}
                  </div>
                  <div className="mt-1 flex items-center gap-1.5 text-[9px] text-muted">
                    <span>{kindMeta[page.kind]?.label ?? page.kind}</span>
                    <span>·</span>
                    <span>{page.sourceIds.length} 来源</span>
                    {page.editedByAuthor ? <Badge tone="amber">作者修订</Badge> : null}
                  </div>
                </button>
              ))}
            </div>
          </aside>

          {current ? (
            <section className="paper-card flex h-full min-h-0 flex-col overflow-hidden">
              <div className="flex items-start justify-between border-b border-line px-6 py-4">
                <div>
                  <div className="flex items-center gap-2">
                    <h2 className="display text-[22px] text-ink">{current.title}</h2>
                    <Badge tone={current.staleAt ? "amber" : "teal"}>
                      {current.staleAt ? "已过期" : "新鲜"}
                    </Badge>
                    {current.editedByAuthor ? <Badge tone="amber">作者修订</Badge> : null}
                    <Badge tone={current.compileMode === "llm" ? "teal" : "neutral"}>
                      {current.compileMode === "llm"
                        ? "模型摘要"
                        : current.compileMode === "author"
                          ? "作者编译"
                          : "确定性"}
                    </Badge>
                  </div>
                  <div className="mt-1.5 font-mono text-[9px] text-muted">
                    {kindMeta[current.kind]?.label ?? current.kind} · hash {current.contentHash.slice(0, 16)} · generator v
                    {current.generatorVersion}
                  </div>
                </div>
                <Button
                  size="sm"
                  onClick={() => setEditing({ title: current.title, content: current.content })}
                  icon={<Pencil size={11} />}
                >
                  提出修改
                </Button>
              </div>
              <div className="min-h-0 flex-1 overflow-y-auto px-6 py-5">
                {current.staleAt ? (
                  <div className="mb-4 rounded-lg border border-amber/30 bg-amber-soft/50 px-3.5 py-2.5 text-[10.5px] leading-5 text-amber">
                    源事实已变化。本页只作旧版导航，不应用作事实依据；请全量重建或查看原始证据。
                  </div>
                ) : null}
                {pageEdits.length ? (
                  <div className="mb-4 rounded-xl border border-teal/25 bg-teal-soft/35 p-3.5">
                    <div className="mb-2 flex items-center gap-1.5 text-[10px] font-semibold text-teal">
                      <ShieldCheck size={12} /> 待确认的作者修改
                    </div>
                    {pageEdits.map((candidate) => (
                      <div key={candidate.id} className="flex items-center justify-between gap-3 py-1">
                        <span className="line-clamp-1 text-[10px] text-ink-soft">
                          {String(candidate.payload.title ?? "")}
                        </span>
                        <Button size="sm" tone="primary" onClick={() => void confirmEdit(candidate)}>
                          确认应用
                        </Button>
                      </div>
                    ))}
                  </div>
                ) : null}
                <Markdown className="text-[12.5px] leading-7 text-ink-soft">{current.content}</Markdown>
                <div className="mt-6 border-t border-line pt-4">
                  <div className="mb-2 flex items-center gap-1.5 text-[10px] font-semibold text-ink-soft">
                    <BookOpen size={12} className="text-teal" /> 来源
                  </div>
                  <div className="flex flex-wrap gap-1.5">
                    {current.sourceSpans.map((span, index) => (
                      <button
                        key={`${span}-${index}`}
                        onClick={() => setSource(span)}
                        className="max-w-full truncate rounded-md bg-paper px-2 py-1 font-mono text-[8.5px] text-ink-soft hover:text-teal"
                        title="查看原文片段"
                      >
                        {span.length > 48 ? `${span.slice(0, 48)}…` : span}
                      </button>
                    ))}
                    {current.sourceIds.map((id, index) => (
                      <button
                        key={`${id}-${index}`}
                        onClick={() => setSource(id)}
                        className="max-w-full truncate rounded-md border border-line bg-paper-raised px-2 py-1 font-mono text-[8.5px] text-muted hover:text-teal"
                      >
                        {id}
                      </button>
                    ))}
                  </div>
                </div>
                <div className="mt-5 border-t border-line pt-4">
                  <div className="mb-2 flex items-center gap-1.5 text-[10px] font-semibold text-ink-soft">
                    <BookOpen size={12} className="text-teal" /> 页面依赖
                  </div>
                  {pageDependencies.length ? (
                    <div className="space-y-1.5">
                      {pageDependencies.map((dependency) => {
                        const target = pages.find(
                          (page) => page.id === dependency.dependsOnPageId,
                        );
                        return (
                          <button
                            key={dependency.dependsOnPageId}
                            onClick={() => {
                              setKind("all");
                              setSelectedId(dependency.dependsOnPageId);
                            }}
                            className="flex w-full items-center justify-between rounded-lg border border-line bg-paper px-3 py-2 text-left hover:border-teal/30"
                          >
                            <span className="text-[10px] text-ink-soft">
                              {target?.title ?? dependency.dependsOnPageId}
                            </span>
                            <span className="text-[8.5px] text-muted">{dependency.reason}</span>
                          </button>
                        );
                      })}
                    </div>
                  ) : (
                    <div className="text-[9.5px] text-muted">当前页面没有可解析的页间依赖。</div>
                  )}
                </div>
              </div>
            </section>
          ) : null}
        </div>
      )}

      <Modal
        open={Boolean(editing)}
        onOpenChange={(open) => !open && setEditing(null)}
        title="提出百科修改"
        description="修改先进入候选区。确认后只更新派生页面，不会反向覆盖人物、事实、规则或正文。"
        width="max-w-[760px]"
        footer={
          <>
            <Button tone="ghost" onClick={() => setEditing(null)}>
              取消
            </Button>
            <Button
              tone="primary"
              onClick={() => void proposeEdit()}
              disabled={busy || !editing?.title.trim() || !editing?.content.trim()}
            >
              提出修改候选
            </Button>
          </>
        }
      >
        {editing ? (
          <div className="space-y-3">
            <Field label="页面标题">
              <Input
                value={editing.title}
                onChange={(e) => setEditing({ ...editing, title: e.target.value })}
              />
            </Field>
            <Field label="派生页面内容" hint="未确认前不会显示为已应用内容">
              <Textarea
                rows={14}
                value={editing.content}
                onChange={(e) => setEditing({ ...editing, content: e.target.value })}
              />
            </Field>
          </div>
        ) : null}
      </Modal>

      <Modal
        open={Boolean(source)}
        onOpenChange={(open) => !open && setSource(null)}
        title="来源定位"
        description="这里显示页面保留的来源 ID 或原文片段；百科文本本身不是事实源。"
        footer={
          <Button tone="ghost" onClick={() => setSource(null)}>
            关闭
          </Button>
        }
      >
        <pre className="max-h-[360px] overflow-auto whitespace-pre-wrap rounded-lg border border-line bg-paper p-3 font-mono text-[10px] leading-5 text-ink-soft">
          {source}
        </pre>
      </Modal>
    </div>
  );
}
