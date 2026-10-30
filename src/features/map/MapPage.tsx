import { Markdown } from "@/components/Markdown";
import { useCallback, useEffect, useRef, useState } from "react";
import cytoscape from "cytoscape";
import {
  GitBranch,
  LoaderCircle,
  Network,
  Radar,
  RefreshCw,
  Sparkles,
} from "lucide-react";
import { Badge, Button, EmptyState, Input, Modal, SectionTitle } from "@/components/ui";
import {
  ipc,
  type CommunityReport,
  type GraphData,
  type HybridSearchResult,
  type ImpactSet,
  type InsightResult,
  type TimelineItem,
} from "@/lib/ipc";
import { useAppStore } from "@/stores/ui";

type Tab = "timeline" | "graph" | "insights";

export function MapPage() {
  const [tab, setTab] = useState<Tab>("timeline");
  const [timeline, setTimeline] = useState<TimelineItem[]>([]);
  const [graph, setGraph] = useState<GraphData | null>(null);
  const [communities, setCommunities] = useState<CommunityReport[]>([]);
  const [impact, setImpact] = useState<ImpactSet | null>(null);
  const [question, setQuestion] = useState("");
  const [insight, setInsight] = useState<InsightResult | null>(null);
  const [searchQuery, setSearchQuery] = useState("");
  const [hybrid, setHybrid] = useState<HybridSearchResult | null>(null);
  const [searchBusy, setSearchBusy] = useState(false);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const notify = useAppStore((s) => s.notify);

  const load = useCallback(async () => {
    try {
      const [items, graphData, reports] = await Promise.all([
        ipc.timeline(),
        ipc.graphData(),
        ipc.communities(),
      ]);
      setTimeline(items);
      setGraph(graphData);
      setCommunities(reports);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const buildCommunities = async () => {
    setBusy(true);
    setError(null);
    try {
      setCommunities(await ipc.communityBuild());
      setGraph(await ipc.graphData());
      notify("层级社区已增量更新；未变化的报告会保留稳定 ID");
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const summarizeCommunities = async () => {
    setBusy(true);
    setError(null);
    try {
      const result = await ipc.communitySummarize();
      setCommunities(result.reports);
      notify(
        `模型摘要：新编译 ${result.compiled} 个，保留 ${result.preserved} 个，剩余 ${result.remaining} 个`,
      );
      if (result.failed.length) {
        setError(`部分社区未更新：${result.failed.slice(0, 3).join("；")}`);
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const query = async () => {
    if (!question.trim()) return;
    setBusy(true);
    try {
      setInsight(await ipc.insightQuery(question.trim()));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const searchEvidence = async () => {
    if (!searchQuery.trim()) return;
    setSearchBusy(true);
    try {
      setHybrid(await ipc.hybridSearch(searchQuery.trim(), 12));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setSearchBusy(false);
    }
  };

  const inspectNode = async (id: string) => {
    try {
      setImpact(await ipc.impactAnalysis(id));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const storyItems = timeline.filter((item) => item.storyTime);
  const revealItems = timeline.filter((item) => item.revealTime);

  return (
    <div className="fade-in mx-auto max-w-[1240px] px-8 py-7">
      <SectionTitle
        eyebrow="Map · 双时间叙事图"
        title="时间、关系与全书洞察"
        action={
          <Button size="sm" onClick={() => void load()} icon={<RefreshCw size={12} />}>
            刷新
          </Button>
        }
      />
      <div className="mb-5 flex items-center gap-1 border-b border-line">
        {[
          { key: "timeline" as const, label: "双时间线", icon: GitBranch },
          { key: "graph" as const, label: "关系图", icon: Network },
          { key: "insights" as const, label: "全书洞察", icon: Radar },
        ].map((item) => {
          const Icon = item.icon;
          return (
            <button
              key={item.key}
              onClick={() => setTab(item.key)}
              className={`relative flex h-10 items-center gap-2 px-4 text-[12px] font-medium transition ${
                tab === item.key ? "text-teal" : "text-muted hover:text-ink"
              }`}
            >
              <Icon size={14} />
              {item.label}
              {tab === item.key ? (
                <span className="tab-thread" aria-hidden />
              ) : null}
            </button>
          );
        })}
      </div>

      {error ? (
        <div className="mb-4 rounded-lg border border-coral/25 bg-coral-soft/60 px-4 py-2.5 text-[11.5px] text-coral">
          {error}
        </div>
      ) : null}
      {loading ? (
        <div className="flex h-64 items-center justify-center text-[12px] text-muted">
          <LoaderCircle size={15} className="mr-2 animate-spin" /> 正在读取叙事图
        </div>
      ) : null}

      {!loading && tab === "timeline" ? (
        <div className="grid grid-cols-2 gap-5">
          <section className="paper-card min-h-[520px] p-5">
            <div className="mb-4 flex items-center justify-between">
              <div>
                <div className="text-[10px] font-semibold uppercase tracking-[0.18em] text-teal">
                  Story time
                </div>
                <h2 className="display mt-1 text-[20px] text-ink">故事时间</h2>
              </div>
              <Badge tone="teal">{storyItems.length} 项</Badge>
            </div>
            <TimelineColumn items={storyItems} mode="story" />
          </section>
          <section className="paper-card min-h-[520px] p-5">
            <div className="mb-4 flex items-center justify-between">
              <div>
                <div className="text-[10px] font-semibold uppercase tracking-[0.18em] text-amber">
                  Reveal time
                </div>
                <h2 className="display mt-1 text-[20px] text-ink">叙述时间</h2>
              </div>
              <Badge tone="amber">{revealItems.length} 项</Badge>
            </div>
            <TimelineColumn items={revealItems} mode="reveal" />
          </section>
        </div>
      ) : null}

      {!loading && tab === "graph" ? (
        <GraphView graph={graph} onNode={inspectNode} communities={communities.length} />
      ) : null}

      {!loading && tab === "insights" ? (
        <div className="space-y-4">
          <section className="paper-card p-5">
            <div className="flex items-start justify-between gap-5">
              <div>
                <div className="flex items-center gap-2 text-[14px] font-semibold text-ink">
                  <Sparkles size={15} className="text-teal" />
                  GraphRAG 全书问答
                </div>
                <p className="mt-1.5 max-w-2xl text-[11px] leading-5 text-muted">
                  先按实体别名与图谱 PPR 选子图，再融合 FTS、向量与社区证据。报告是派生摘要，不是权威事实。
                </p>
              </div>
              <div className="flex gap-2">
                <Button size="sm" onClick={() => void buildCommunities()} disabled={busy}>
                  {busy ? <LoaderCircle size={12} className="animate-spin" /> : null}
                  增量重建
                </Button>
                <Button
                  size="sm"
                  tone="primary"
                  onClick={() => void summarizeCommunities()}
                  disabled={busy || communities.length === 0}
                >
                  模型摘要
                </Button>
              </div>
            </div>
            <div className="mt-4 flex gap-2">
              <Input
                value={question}
                onChange={(e) => setQuestion(e.target.value)}
                placeholder="例如：哪些关系正在推动当前局面？哪些伏笔仍形成叙事债务？"
                onKeyDown={(e) => e.key === "Enter" && void query()}
              />
              <Button tone="primary" onClick={() => void query()} disabled={busy || !question.trim()}>
                分析
              </Button>
            </div>
            {insight ? (
              <div className="mt-4 rounded-xl border border-teal/20 bg-teal-soft/30 p-4">
                <div className="flex items-center justify-between">
                  <div className="text-[10px] font-semibold tracking-wide text-teal">
                    派生分析 · 非权威事实
                  </div>
                  <Badge tone={insight.retrievalMode === "hybrid" ? "teal" : "neutral"}>
                    {insight.retrievalMode === "hybrid" ? "混合子图检索" : "FTS + PPR"}
                  </Badge>
                </div>
                <Markdown className="mt-2 text-[12px] leading-7 text-ink-soft">{insight.answer}</Markdown>
                {insight.embeddingError ? (
                  <div className="mt-3 rounded-lg bg-amber-soft/40 px-3 py-2 text-[9.5px] leading-5 text-amber">
                    查询向量不可用，已降级：{insight.embeddingError}
                  </div>
                ) : null}
                <div className="mt-3 text-[9px] text-muted">
                  {insight.entityIds.length} 个实体种子 · {insight.communityIds.length} 个社区 ·{" "}
                  {insight.sourceIds.length} 个来源
                </div>
                <div className="mt-3 flex flex-wrap gap-1">
                  {insight.sourceIds.map((source) => (
                    <span
                      key={source}
                      className="max-w-[300px] truncate rounded bg-paper-raised px-1.5 py-0.5 font-mono text-[9px] text-muted"
                      title={source}
                    >
                      {source}
                    </span>
                  ))}
                </div>
              </div>
            ) : null}
          </section>

          <section className="paper-card p-5">
            <div className="flex items-center justify-between gap-4">
              <div>
                <div className="text-[13px] font-semibold text-ink">证据检索</div>
                <div className="mt-1 text-[10.5px] leading-5 text-muted">
                  FTS5、向量召回与 PPR 按 RRF 融合；向量不可用时自动降级。
                </div>
              </div>
              {hybrid ? (
                <Badge tone={hybrid.retrievalMode === "hybrid" ? "teal" : "neutral"}>
                  {hybrid.retrievalMode === "hybrid" ? "混合检索" : "FTS + PPR"}
                </Badge>
              ) : null}
            </div>
            <div className="mt-3 flex gap-2">
              <Input
                value={searchQuery}
                onChange={(event) => setSearchQuery(event.target.value)}
                placeholder="例如：阿澈为什么不再相信守夜人？"
                onKeyDown={(event) => event.key === "Enter" && void searchEvidence()}
              />
              <Button
                tone="primary"
                onClick={() => void searchEvidence()}
                disabled={searchBusy || !searchQuery.trim()}
                className="shrink-0 whitespace-nowrap"
              >
                {searchBusy ? <LoaderCircle size={12} className="animate-spin" /> : null}
                检索证据
              </Button>
            </div>
            {hybrid?.embeddingError ? (
              <div className="mt-3 rounded-lg bg-amber-soft/40 px-3 py-2 text-[9.5px] leading-5 text-amber">
                向量召回已降级：{hybrid.embeddingError}
              </div>
            ) : null}
            {hybrid ? (
              <div className="mt-3 space-y-2">
                {hybrid.hits.length === 0 ? (
                  <div className="py-6 text-center text-[10.5px] text-muted">没有召回证据</div>
                ) : (
                  hybrid.hits.map((hit) => {
                    const channels = [
                      hit.ftsRank ? "FTS" : null,
                      hit.vectorRank ? "向量" : null,
                      hit.graphRank ? "图谱" : null,
                    ]
                      .filter(Boolean)
                      .join(" + ");
                    return (
                      <article
                        key={`${hit.sourceKind}-${hit.sourceId}`}
                        className="rounded-lg border border-line bg-paper px-3 py-2.5"
                      >
                        <div className="flex items-center justify-between gap-3">
                          <span className="font-mono text-[8.5px] text-muted">
                            {hit.sourceKind}:{hit.sourceId}
                          </span>
                          <span className="text-[8.5px] text-teal">
                            {channels || "候选"} · RRF {hit.score}
                          </span>
                        </div>
                        <p className="mt-1 line-clamp-3 text-[10.5px] leading-5 text-ink-soft">
                          {hit.excerpt}
                        </p>
                      </article>
                    );
                  })
                )}
              </div>
            ) : null}
          </section>

          {communities.length === 0 ? (
            <EmptyState
              icon={<Radar size={20} />}
              title="尚无社区报告"
              description="建立关系与已确认事实后，系统会执行分层加权社区发现，并为每个社区生成带来源的派生摘要。"
              action={
                <Button onClick={() => void buildCommunities()} disabled={busy}>
                  建立社区摘要
                </Button>
              }
            />
          ) : (
            <div className="grid grid-cols-2 gap-3">
              {communities.map((report) => (
                <article key={report.id} className="paper-card p-4">
                  <div className="flex items-center justify-between">
                    <div className="text-[12px] font-semibold text-ink">{report.title}</div>
                    <div className="flex gap-1">
                      <Badge tone="neutral">L{report.level}</Badge>
                      <Badge tone={report.summaryMode === "llm" ? "teal" : "neutral"}>
                        {report.summaryMode === "llm" ? "模型摘要" : "确定性"}
                      </Badge>
                      {report.staleAt ? <Badge tone="amber">已过期</Badge> : null}
                    </div>
                  </div>
                  <p className="mt-2 line-clamp-5 whitespace-pre-wrap text-[10.5px] leading-5 text-ink-soft">
                    {report.content}
                  </p>
                  <div className="mt-2 flex flex-wrap gap-1">
                    {report.sourceIds.slice(0, 6).map((source) => (
                      <span key={source} className="rounded bg-paper px-1 py-0.5 font-mono text-[8.5px] text-muted">
                        {source.length > 24 ? `${source.slice(0, 24)}…` : source}
                      </span>
                    ))}
                  </div>
                </article>
              ))}
            </div>
          )}
        </div>
      ) : null}

      <Modal
        open={Boolean(impact)}
        onOpenChange={(open) => !open && setImpact(null)}
        title={`影响集 · ${impact?.targetLabel ?? ""}`}
        description="修改该实体前，先查看会受影响的事实、关系、事件、场景与冲突。"
        width="max-w-[680px]"
        footer={
          <Button tone="ghost" onClick={() => setImpact(null)}>
            关闭
          </Button>
        }
      >
        <div className="space-y-2">
          {impact?.items.map((item) => (
            <div key={`${item.kind}-${item.id}`} className="rounded-lg border border-line bg-paper/60 px-3 py-2.5">
              <div className="flex items-center gap-2">
                <Badge tone="neutral">{item.kind}</Badge>
                <span className="text-[11px] font-medium text-ink">{item.label}</span>
              </div>
              <div className="mt-1 text-[9.5px] text-muted">{item.reason}</div>
              {item.source ? (
                <div className="mt-1 truncate font-mono text-[8.5px] text-ink-soft">{item.source}</div>
              ) : null}
            </div>
          ))}
          {impact && !impact.items.length ? (
            <div className="py-8 text-center text-[11px] text-muted">暂无下游影响</div>
          ) : null}
        </div>
      </Modal>
    </div>
  );
}

function TimelineColumn({ items, mode }: { items: TimelineItem[]; mode: "story" | "reveal" }) {
  if (!items.length) {
    return (
      <div className="flex h-[380px] items-center justify-center text-center text-[11px] leading-6 text-muted">
        还没有带{mode === "story" ? "故事时间" : "叙述时间"}的事件或事实。
      </div>
    );
  }
  return (
    <div className="relative space-y-3 pl-4 before:absolute before:bottom-4 before:left-[5px] before:top-1 before:w-px before:bg-line">
      {items.map((item) => (
        <div key={`${mode}-${item.id}`} className="relative">
          <span className="absolute -left-4 top-2 h-2.5 w-2.5 rounded-full border-2 border-paper-raised bg-teal" />
          <div className="rounded-lg border border-line bg-paper/60 px-3 py-2.5">
            <div className="flex items-center justify-between gap-2">
              <span className="text-[11.5px] font-medium text-ink">{item.label}</span>
              <span className="shrink-0 text-[9px] text-muted">
                {mode === "story" ? item.storyTime : item.revealTime}
              </span>
            </div>
            <div className="mt-1 flex items-center gap-2 text-[9px] text-muted">
              <span>{item.kind === "event" ? "事件" : "事实"}</span>
              {item.branchId ? <Badge tone="neutral">分支事实</Badge> : null}
            </div>
            {item.detail ? (
              <div className="mt-1 line-clamp-2 text-[9.5px] leading-4 text-muted">{item.detail}</div>
            ) : null}
            {item.source ? (
              <div className="mt-1 truncate font-mono text-[8.5px] text-ink-soft">{item.source}</div>
            ) : null}
          </div>
        </div>
      ))}
    </div>
  );
}

function GraphView({
  graph,
  onNode,
  communities,
}: {
  graph: GraphData | null;
  onNode: (id: string) => void;
  communities: number;
}) {
  const container = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!container.current || !graph) return;
    const cy = cytoscape({
      container: container.current,
      pixelRatio: 1,
      elements: [
        ...graph.nodes.map((node) => ({
          data: { id: node.id, label: node.label, kind: node.kind },
        })),
        ...graph.edges.map((edge) => ({
          data: { id: edge.id, source: edge.from, target: edge.to, label: edge.label },
        })),
      ],
      style: [
        {
          selector: "node",
          style: {
            "background-color": "#2f5e3f",
            label: "data(label)",
            color: "#20271f",
            "font-size": "11px",
            "text-valign": "bottom",
            "text-margin-y": 5,
            width: "38px",
            height: "38px",
          },
        },
        {
          selector: 'node[kind = "place"]',
          style: { "background-color": "#8a5c0c" },
        },
        {
          selector: 'node[kind = "item"]',
          style: { "background-color": "#a9452a" },
        },
        {
          selector: "edge",
          style: {
            width: 1,
            "line-color": "#b9c9ac",
            "target-arrow-shape": "none",
            label: "data(label)",
            "font-size": "9px",
            "text-background-color": "#f3efe2",
            "text-background-opacity": 0.9,
            "text-background-padding": "2px",
            color: "#5f6a5a",
          },
        },
      ],
      layout: { name: "cose", animate: false, fit: false, padding: 30 },
    });
    cy.zoom(1);
    cy.center();
    const centerTimer = window.setTimeout(() => {
      cy.fit(undefined, 100);
      if (cy.zoom() > 1.25) cy.zoom(1.25);
      cy.center();
    }, 0);
    cy.on("tap", "node", (event) => onNode(event.target.id()));
    return () => {
      window.clearTimeout(centerTimer);
      cy.destroy();
    };
  }, [graph, onNode]);

  return (
    <section className="paper-card overflow-hidden">
      <div className="flex items-center justify-between border-b border-line px-5 py-3">
        <div>
          <div className="text-[12px] font-semibold text-ink">关系图</div>
          <div className="mt-0.5 text-[9.5px] text-muted">
            {graph?.nodes.length ?? 0} 个实体 · {graph?.edges.length ?? 0} 条边 · {communities} 个社区
          </div>
        </div>
        <span className="text-[9.5px] text-muted">点击节点查看影响集</span>
      </div>
      {!graph?.nodes.length ? (
        <div className="flex h-[520px] items-center justify-center text-[11px] text-muted">
          还没有已确认实体与关系
        </div>
      ) : (
        <div ref={container} className="h-[560px] w-full bg-paper" />
      )}
    </section>
  );
}
