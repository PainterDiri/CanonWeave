import { useCallback, useEffect, useMemo, useState } from "react";
import {
  AlertTriangle,
  ArrowRight,
  Check,
  CircleDot,
  Compass,
  GitBranch,
  LoaderCircle,
  Pencil,
  RefreshCw,
  ShieldCheck,
  Sparkles,
  UsersRound,
} from "lucide-react";
import { Badge, Button, Field, Input, Modal, SectionTitle, Textarea } from "@/components/ui";
import { AgentSuggestion } from "@/components/AgentSuggestion";
import {
  ipc,
  onBlueprintProgress,
  type NarrativeBlueprint,
  type NarrativeBlueprintEditInput,
  type NarrativeBlueprintView,
  type NarrativeElement,
  type NarrativeLogicEdge,
  type NarrativeLogicNode,
  type NarrativeTimelineEvent,
} from "@/lib/ipc";
import { useAppStore } from "@/stores/ui";
import { LogicGraph } from "./LogicGraph";

type Medium = "novel" | "game";
type Strategy = "adaptive" | "fixed";

const mediums: { key: Medium; label: string; hint: string }[] = [
  { key: "novel", label: "小说", hint: "以章节、场景、叙述者与对白组织" },
  { key: "game", label: "游戏", hint: "以任务、区域、对白、交互与演出组织" },
];

const strategies: { key: Strategy; label: string; hint: string }[] = [
  { key: "adaptive", label: "逻辑链决定体量", hint: "适合小说：先评估需要多少章节，再随连载调整" },
  { key: "fixed", label: "固定章节/任务数", hint: "适合游戏或有硬性篇幅要求的作品" },
];

export function BlueprintPanel({
  hasPremise,
  onNeedPremise,
  onReady,
}: {
  hasPremise: boolean;
  onNeedPremise: () => void;
  onReady: () => void;
}) {
  const [view, setView] = useState<NarrativeBlueprintView | null>(null);
  const [medium, setMedium] = useState<Medium>("novel");
  const [strategy, setStrategy] = useState<Strategy>("adaptive");
  const [depth, setDepth] = useState<"epic" | "standard" | "quick">("epic");
  const [targetUnits, setTargetUnits] = useState(60);
  const [loading, setLoading] = useState(true);
  const [generating, setGenerating] = useState(false);
  const [progressLabel, setProgressLabel] = useState<string | null>(null);

  useEffect(() => {
    let dispose = () => {};
    void onBlueprintProgress((event) => setProgressLabel(`${event.label}（${event.step}/${event.total}）`)).then(
      (unlisten) => {
        dispose = unlisten;
      },
    );
    return () => dispose();
  }, []);
  const [confirming, setConfirming] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [nodeOpen, setNodeOpen] = useState(false);
  const [coreOpen, setCoreOpen] = useState(false);
  const [saving, setSaving] = useState(false);
  const [elementEditing, setElementEditing] = useState<NarrativeElement | null>(null);
  const [timelineEditing, setTimelineEditing] = useState<NarrativeTimelineEvent | null>(null);
  const [nodeEditing, setNodeEditing] = useState<NarrativeLogicNode | null>(null);
  const [edgeEditing, setEdgeEditing] = useState<NarrativeLogicEdge | null>(null);
  const [connecting, setConnecting] = useState(false);
  const [connectSource, setConnectSource] = useState<string | null>(null);
  const refreshProject = useAppStore((state) => state.refreshProject);

  const load = useCallback(async () => {
    try {
      const current = await ipc.blueprintCurrent();
      const candidate = current ?? (await ipc.blueprintCandidate());
      setView(candidate);
      if (candidate) {
        setMedium(candidate.blueprint.medium === "game" ? "game" : "novel");
        setStrategy(candidate.blueprint.structureStrategy === "fixed" ? "fixed" : "adaptive");
        if (candidate.blueprint.targetUnits) setTargetUnits(candidate.blueprint.targetUnits);
      }
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const generate = async () => {
    if (!hasPremise) {
      onNeedPremise();
      return;
    }
    setGenerating(true);
    setProgressLabel(null);
    setError(null);
    try {
      const next = await ipc.blueprintGenerate({
        medium,
        structureStrategy: strategy,
        targetUnits: strategy === "fixed" ? targetUnits : null,
        depth,
      });
      setView(next);
      await refreshProject();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setGenerating(false);
    }
  };

  const confirm = async () => {
    if (!view) return;
    setConfirming(true);
    setError(null);
    try {
      const next = await ipc.blueprintConfirm(view.blueprint.id);
      setView(next);
      await refreshProject();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setConfirming(false);
    }
  };

  const persist = async (next: NarrativeBlueprintView) => {
    setSaving(true);
    setError(null);
    try {
      const updated = await ipc.blueprintUpdate(toEditInput(next));
      setView(updated);
      await refreshProject();
      return updated;
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
      throw cause;
    } finally {
      setSaving(false);
    }
  };

  const toggleConnecting = () => {
    setConnecting((current) => !current);
    setConnectSource(null);
  };

  const handleGraphNode = (nodeId: string) => {
    if (!connecting) return;
    if (!connectSource) {
      setConnectSource(nodeId);
      return;
    }
    if (connectSource === nodeId) {
      setConnectSource(null);
      return;
    }
    if (!view) return;
    const from = view.nodes.find((node) => node.id === connectSource);
    const to = view.nodes.find((node) => node.id === nodeId);
    if (!from || !to) return;
    setEdgeEditing({
      id: "",
      fromNodeId: from.id,
      fromTitle: from.title,
      toNodeId: to.id,
      toTitle: to.title,
      relation: "导致",
      condition: "",
      status: "candidate",
    });
    setConnecting(false);
    setConnectSource(null);
  };

  const saveEdge = async (edge: NarrativeLogicEdge) => {
    if (!view) return;
    const exists = view.edges.some((item) => item.id === edge.id && edge.id !== "");
    const edges = exists
      ? view.edges.map((item) => (item.id === edge.id ? edge : item))
      : [...view.edges, edge];
    await persist({ ...view, edges });
    setEdgeEditing(null);
  };

  const deleteEdge = async (edge: NarrativeLogicEdge) => {
    if (!view) return;
    await persist({ ...view, edges: view.edges.filter((item) => item.id !== edge.id) });
    setEdgeEditing(null);
  };

  const diagnostics = view?.diagnostics ?? [];
  const blocking = diagnostics.filter((item) => item.severity === "error");
  const warnings = diagnostics.filter((item) => item.severity === "warning");
  const confirmed = view?.blueprint.status === "confirmed";
  const characters = useMemo(
    () => view?.elements.filter((item) => item.kind === "character") ?? [],
    [view],
  );
  const worldElements = useMemo(
    () => view?.elements.filter((item) => item.kind !== "character") ?? [],
    [view],
  );

  if (loading) {
    return (
      <div className="flex h-64 items-center justify-center text-[12px] text-muted">
        <LoaderCircle size={15} className="mr-2 animate-spin" /> 正在读取叙事蓝本
      </div>
    );
  }

  return (
    <div className="space-y-5">
      <AgentSuggestion outputs={["all", "outline", "knowledge"]} />
      {!confirmed ? null : (
        <section className="rounded-xl border border-teal/25 bg-teal-soft/40 px-4 py-3">
          <div className="flex items-center gap-2 text-[12px] text-teal">
            <ShieldCheck size={15} /> 叙事蓝本已确认，章节与正文现在可以引用完整逻辑链。
          </div>
        </section>
      )}

      {error ? (
        <div className="rounded-lg border border-coral/25 bg-coral-soft/60 px-4 py-2.5 text-[12px] leading-5 text-coral">
          {error}
        </div>
      ) : null}

      {!view ? (
        <section className="paper-card p-6">
          <SectionTitle
            eyebrow="Blueprint · 地基"
            title="先建立叙事地基，再谈章节"
            action={hasPremise ? <Badge tone="teal">种子卡已就绪</Badge> : <Badge tone="amber">缺少种子卡</Badge>}
          />
          <p className="mt-2 max-w-3xl text-[12px] leading-6 text-muted">
            CanonWeave 会先生成人物与势力的欲望冲突、世界规则的代价与边界、时间线因果，以及可验证的叙事线。
            这些内容确认后，逻辑链才有资格决定章节或任务数量。
          </p>
          <div className="mt-5 grid grid-cols-2 gap-3">
            {mediums.map((item) => (
              <button
                key={item.key}
                onClick={() => setMedium(item.key)}
                className={`rounded-xl border p-4 text-left transition ${
                  medium === item.key
                    ? "border-teal bg-teal-soft/40"
                    : "border-line bg-paper/50 hover:border-teal/40"
                }`}
              >
                <div className="text-[12px] font-semibold text-ink">{item.label}</div>
                <div className="mt-1 text-[10.5px] leading-5 text-muted">{item.hint}</div>
              </button>
            ))}
          </div>
          <div className="mt-3 grid grid-cols-2 gap-3">
            {strategies.map((item) => (
              <button
                key={item.key}
                onClick={() => setStrategy(item.key)}
                className={`rounded-xl border p-4 text-left transition ${
                  strategy === item.key
                    ? "border-teal bg-teal-soft/40"
                    : "border-line bg-paper/50 hover:border-teal/40"
                }`}
              >
                <div className="text-[12px] font-semibold text-ink">{item.label}</div>
                <div className="mt-1 text-[10.5px] leading-5 text-muted">{item.hint}</div>
              </button>
            ))}
          </div>
          <div className="mt-3 grid grid-cols-3 gap-3" role="radiogroup" aria-label="地基规模">
            {(
              [
                ["epic", "史诗级地基", "分层构建：世界与能力体系 → 势力 → 人物群像 → 暗线 → 漏洞审查。适合长篇连载，约 7 次模型调用。"],
                ["standard", "标准地基", "同样分层，但规模较小，适合中篇或单卷故事。"],
                ["quick", "快速草案", "一次生成，只用于试方向。"],
              ] as const
            ).map(([key, label, hint]) => (
              <button
                key={key}
                role="radio"
                aria-checked={depth === key}
                onClick={() => setDepth(key)}
                className={`rounded-xl border p-3.5 text-left transition ${
                  depth === key ? "border-teal bg-teal-soft/40" : "border-line bg-paper/50 hover:border-teal/40"
                }`}
              >
                <div className="text-[12px] font-semibold text-ink">{label}</div>
                <div className="mt-1 text-[10.5px] leading-5 text-muted">{hint}</div>
              </button>
            ))}
          </div>
          {strategy === "fixed" ? (
            <div className="mt-3 max-w-[260px]">
              <Field label="目标章节 / 任务数">
                <Input
                  type="number"
                  min={1}
                  max={1200}
                  value={targetUnits}
                  onChange={(event) => setTargetUnits(Number(event.target.value))}
                />
              </Field>
            </div>
          ) : null}
          <div className="mt-5 flex items-center gap-2">
            <Button
              tone="primary"
              onClick={() => void generate()}
              disabled={generating || !hasPremise}
              icon={generating ? <LoaderCircle size={14} className="animate-spin" /> : <Sparkles size={14} />}
            >
              {generating ? progressLabel ?? "正在建立地基与逻辑链" : "生成叙事蓝本"}
            </Button>
            {!hasPremise ? (
              <Button tone="ghost" onClick={onNeedPremise}>
                先确认种子卡
              </Button>
            ) : null}
          </div>
        </section>
      ) : (
        <>
          <section className="paper-card p-6">
            <div className="flex items-start justify-between gap-5">
              <div>
                <div className="flex items-center gap-2">
                  <Compass size={16} className="text-teal" />
                  <h2 className="display text-[22px] text-ink">{view.blueprint.title}</h2>
                  <Badge tone={confirmed ? "teal" : "amber"}>
                    {confirmed ? "已确认" : "候选"}
                  </Badge>
                </div>
                <p className="mt-2 max-w-3xl text-[12px] leading-6 text-ink-soft">
                  {view.blueprint.coreDramaticQuestion}
                </p>
              </div>
              <div className="shrink-0 text-right">
                <div className="text-[26px] font-semibold tracking-[-0.04em] text-teal">
                  {view.blueprint.completeness}
                </div>
                <div className="text-[9px] uppercase tracking-[0.14em] text-muted">完整度</div>
              </div>
            </div>

            <div className="mt-5 grid grid-cols-3 gap-3">
              <Metric label="开局长板" value={view.blueprint.openingState} />
              <Metric label="终局状态" value={view.blueprint.endingState} />
              <Metric label="叙事承诺" value={view.blueprint.narrativePromise} />
            </div>

            <div className="mt-4 flex flex-wrap items-center gap-2 border-t border-line pt-4">
              <Badge tone="teal">{view.blueprint.medium === "game" ? "游戏" : "小说"}</Badge>
              <Badge tone="neutral">
                {view.blueprint.structureStrategy === "adaptive" ? "体量由逻辑链决定" : `固定 ${view.blueprint.targetUnits ?? 0} 章/任务`}
              </Badge>
              <span className="text-[10.5px] text-muted">
                {view.threads.length} 条叙事线 · {view.nodes.length} 个逻辑节点 · {view.edges.length} 条因果边
              </span>
              {!confirmed ? (
                <>
                  <Button
                    size="sm"
                    tone="ghost"
                    className="ml-auto"
                    onClick={() => setCoreOpen(true)}
                    icon={<Pencil size={12} />}
                  >
                    编辑核心契约
                  </Button>
                  <Button
                    size="sm"
                    tone="primary"
                    onClick={() => void confirm()}
                    disabled={confirming || blocking.length > 0 || saving}
                    icon={confirming ? <LoaderCircle size={12} className="animate-spin" /> : <Check size={12} />}
                  >
                    确认蓝本
                  </Button>
                </>
              ) : (
                <Button size="sm" tone="ghost" className="ml-auto" onClick={onReady} icon={<ArrowRight size={12} />}>
                  进入卷章编排
                </Button>
              )}
            </div>
          </section>

          {diagnostics.length > 0 ? (
            <section className="paper-card p-5">
              <div className="mb-3 flex items-center gap-2">
                <AlertTriangle size={15} className={blocking.length ? "text-coral" : "text-amber"} />
                <h3 className="text-[13px] font-semibold text-ink">
                  逻辑完整性检查 · {blocking.length} 项阻断 / {warnings.length} 项建议
                </h3>
              </div>
              <div className="space-y-2">
                {diagnostics.map((item, index) => (
                  <div
                    key={`${item.code}-${index}`}
                    className={`rounded-lg border px-3 py-2 text-[10.5px] leading-5 ${
                      item.severity === "error"
                        ? "border-coral/20 bg-coral-soft/40 text-coral"
                        : "border-amber/20 bg-amber-soft/40 text-amber"
                    }`}
                  >
                    <b>{item.path}</b> · {item.message}
                  </div>
                ))}
              </div>
            </section>
          ) : null}

          <div className="grid grid-cols-2 gap-4">
            <BlueprintCard icon={<UsersRound size={15} />} title="人物与关系">
              {characters.map((item) => (
                <div key={item.id} className="rounded-lg border border-line bg-paper/40 p-3">
                  <div className="flex items-center gap-2">
                    <b className="text-[12px] text-ink">{item.name}</b>
                    <span className="text-[9.5px] text-muted">{item.role}</span>
                    {!confirmed ? (
                      <EditIcon onClick={() => setElementEditing(item)} label={`编辑 ${item.name}`} />
                    ) : null}
                  </div>
                  <p className="mt-1 text-[10.5px] leading-5 text-ink-soft">{item.summary}</p>
                  <p className="mt-2 text-[10px] leading-5 text-muted">
                    目标：{item.externalGoal || "未设定"}｜需要：{item.internalNeed || "未设定"}｜恐惧：{item.fear || "未设定"}
                  </p>
                </div>
              ))}
            </BlueprintCard>
            <BlueprintCard icon={<GitBranch size={15} />} title="势力、世界规则与时间线">
              {worldElements.map((item) => (
                <div key={item.id} className="rounded-lg border border-line bg-paper/40 p-3">
                  <div className="flex items-center gap-2">
                    <Badge tone="neutral">{item.kind}</Badge>
                    <b className="text-[12px] text-ink">{item.name}</b>
                    {!confirmed ? (
                      <EditIcon onClick={() => setElementEditing(item)} label={`编辑 ${item.name}`} />
                    ) : null}
                  </div>
                  <p className="mt-1 text-[10.5px] leading-5 text-ink-soft">{item.summary}</p>
                </div>
              ))}
              {view.timeline.slice(0, 5).map((event) => (
                <div key={event.id} className="border-l-2 border-teal/30 pl-3">
                  <div className="flex items-center gap-2 text-[11px] font-medium text-ink">
                    <span>{event.orderNo}. {event.title} <span className="text-muted">· {event.storyTime}</span></span>
                    {!confirmed ? (
                      <EditIcon onClick={() => setTimelineEditing(event)} label={`编辑 ${event.title}`} />
                    ) : null}
                  </div>
                  <div className="mt-1 text-[10px] leading-5 text-muted">
                    因：{event.cause}｜果：{event.outcome}｜影响：{event.consequence}
                  </div>
                </div>
              ))}
            </BlueprintCard>
          </div>

          <section className="paper-card p-5">
            <div className="mb-4 flex items-center justify-between gap-3">
              <div className="flex items-center gap-2">
                <CircleDot size={15} className="text-teal" />
                <h3 className="display text-[18px] text-ink">叙事逻辑链</h3>
              </div>
              <div className="flex items-center gap-2">
                {!confirmed ? (
                  <Button
                    size="sm"
                    tone={connecting ? "primary" : "ghost"}
                    onClick={toggleConnecting}
                    icon={<GitBranch size={12} />}
                  >
                    {connecting ? "取消连线" : "新增因果边"}
                  </Button>
                ) : null}
                <Button size="sm" tone="ghost" onClick={() => setNodeOpen(true)}>
                  查看全部节点
                </Button>
              </div>
            </div>
            {connecting ? (
              <div className="mb-3 rounded-lg border border-teal/25 bg-teal-soft/40 px-3 py-2 text-[10.5px] leading-5 text-teal">
                {connectSource
                  ? "已选择起点。请点击另一个节点作为终点；再次点击起点可取消。"
                  : "连线模式：先点击起点节点，再点击终点节点。"}
              </div>
            ) : (
              <p className="mb-3 text-[10px] leading-5 text-muted">
                节点表示因果状态，箭头表示“谁迫使谁”。点击边可编辑关系或删除。
              </p>
            )}
            <div className="mb-4">
              <LogicGraph
                nodes={view.nodes}
                edges={view.edges}
                connecting={connecting}
                sourceId={connectSource}
                selectedEdgeId={edgeEditing?.id || null}
                onNodeClick={handleGraphNode}
                onEdgeClick={(edge) => {
                  if (!confirmed) setEdgeEditing(edge);
                }}
              />
            </div>
            <div className="space-y-3">
              {view.nodes.slice(0, 8).map((node) => (
                <div key={node.id} className="grid grid-cols-[44px_minmax(0,1fr)] gap-3">
                  <div className="flex h-8 w-8 items-center justify-center rounded-full bg-teal-soft text-[11px] font-semibold text-teal">
                    {node.orderNo}
                  </div>
                  <div className="border-b border-line pb-3">
                    <div className="flex flex-wrap items-center gap-2">
                      <b className="text-[12px] text-ink">{node.title}</b>
                      <Badge tone="neutral">{node.kind}</Badge>
                      <span className="text-[9.5px] text-muted">{node.threadTitle || "主线"}</span>
                      {!confirmed ? (
                        <EditIcon onClick={() => setNodeEditing(node)} label={`编辑 ${node.title}`} />
                      ) : null}
                    </div>
                    <p className="mt-1 text-[10.5px] leading-5 text-ink-soft">
                      因：{node.cause} → 选择：{node.choice} → 代价：{node.cost} → 结果：{node.outcome}
                    </p>
                    <p className="mt-1 text-[10px] leading-5 text-muted">
                      状态变化：{node.stateChange}｜认知变化：{node.knowledgeChange}
                    </p>
                  </div>
                </div>
              ))}
            </div>
          </section>

          {view.presentations.length > 0 ? (
            <section className="paper-card p-5">
              <div className="mb-3 flex items-center gap-2">
                <RefreshCw size={15} className="text-amber" />
                <h3 className="display text-[18px] text-ink">游戏呈现契约</h3>
              </div>
              <div className="grid grid-cols-3 gap-2">
                {view.presentations.map((item) => (
                  <div key={item.id} className="rounded-lg border border-line bg-paper/40 p-3">
                    <div className="flex items-center justify-between gap-2">
                      <b className="text-[11px] text-ink">{item.logicNodeTitle}</b>
                      <Badge tone="amber">{item.mode}</Badge>
                    </div>
                    <div className="mt-2 text-[10px] leading-5 text-muted">
                      对白 {item.dialogueRatio}% · 旁白 {item.narrationRatio}%
                    </div>
                    <p className="mt-1 text-[10px] leading-5 text-ink-soft">
                      {item.deliverable || item.playerAgency || item.interaction || "待补充"}
                    </p>
                  </div>
                ))}
              </div>
            </section>
          ) : null}
        </>
      )}

      <Modal
        open={nodeOpen}
        onOpenChange={setNodeOpen}
        title="叙事逻辑链"
        description="节点顺序不是章节顺序；它描述故事世界中必须成立的因果关系。"
        width="max-w-[860px]"
      >
        <div className="space-y-3">
          {(view?.nodes ?? []).map((node) => (
            <div key={node.id} className="rounded-xl border border-line p-4">
              <div className="flex items-center gap-2">
                <span className="text-[11px] font-semibold text-teal">#{node.orderNo}</span>
                <b className="text-[12px] text-ink">{node.title}</b>
                <Badge tone="neutral">{node.kind}</Badge>
              </div>
              <div className="mt-2 grid grid-cols-2 gap-x-5 gap-y-2 text-[10.5px] leading-5">
                <Detail label="前因" value={node.cause} />
                <Detail label="选择" value={node.choice} />
                <Detail label="代价" value={node.cost} />
                <Detail label="结果" value={node.outcome} />
                <Detail label="状态变化" value={node.stateChange} />
                <Detail label="认知变化" value={node.knowledgeChange} />
              </div>
            </div>
          ))}
        </div>
      </Modal>

      <CoreEditor
        view={view}
        open={coreOpen}
        saving={saving}
        onOpenChange={setCoreOpen}
        onSave={async (blueprint) => {
          if (!view) return;
          await persist({ ...view, blueprint: { ...view.blueprint, ...blueprint } });
          setCoreOpen(false);
        }}
      />
      <ElementEditor
        element={elementEditing}
        open={Boolean(elementEditing)}
        saving={saving}
        onOpenChange={(open) => !open && setElementEditing(null)}
        onSave={async (element) => {
          if (!view) return;
          await persist({
            ...view,
            elements: view.elements.map((item) => (item.id === element.id ? element : item)),
          });
          setElementEditing(null);
        }}
      />
      <TimelineEditor
        event={timelineEditing}
        open={Boolean(timelineEditing)}
        saving={saving}
        onOpenChange={(open) => !open && setTimelineEditing(null)}
        onSave={async (event) => {
          if (!view) return;
          await persist({
            ...view,
            timeline: view.timeline.map((item) => (item.id === event.id ? event : item)),
          });
          setTimelineEditing(null);
        }}
      />
      <LogicNodeEditor
        node={nodeEditing}
        open={Boolean(nodeEditing)}
        saving={saving}
        onOpenChange={(open) => !open && setNodeEditing(null)}
        onSave={async (node) => {
          if (!view) return;
          await persist({
            ...view,
            nodes: view.nodes.map((item) => (item.id === node.id ? node : item)),
          });
          setNodeEditing(null);
        }}
      />
      <EdgeEditor
        edge={edgeEditing}
        open={Boolean(edgeEditing)}
        saving={saving}
        onOpenChange={(open) => {
          if (!open) {
            setEdgeEditing(null);
            setConnectSource(null);
          }
        }}
        onSave={saveEdge}
        onDelete={deleteEdge}
      />
    </div>
  );
}

function toEditInput(view: NarrativeBlueprintView): NarrativeBlueprintEditInput {
  const b = view.blueprint;
  return {
    id: b.id,
    title: b.title,
    medium: b.medium,
    structureStrategy: b.structureStrategy,
    targetUnits: b.targetUnits,
    coreDramaticQuestion: b.coreDramaticQuestion,
    narrativePromise: b.narrativePromise,
    openingState: b.openingState,
    endingState: b.endingState,
    themeQuestion: b.themeQuestion,
    elements: view.elements,
    timeline: view.timeline,
    threads: view.threads,
    nodes: view.nodes,
    edges: view.edges,
    presentations: view.presentations,
  };
}

function EditIcon({ onClick, label }: { onClick: () => void; label: string }) {
  return (
    <button
      onClick={onClick}
      className="focus-ring ml-auto flex h-5 w-5 shrink-0 items-center justify-center rounded text-muted hover:bg-ink/5 hover:text-teal"
      aria-label={label}
      title={label}
    >
      <Pencil size={10} />
    </button>
  );
}

function Metric({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-xl border border-line bg-paper/40 p-3">
      <div className="text-[9px] uppercase tracking-[0.12em] text-muted">{label}</div>
      <div className="mt-1 line-clamp-4 text-[10.5px] leading-5 text-ink-soft">{value || "未设定"}</div>
    </div>
  );
}

function BlueprintCard({
  icon,
  title,
  children,
}: {
  icon: React.ReactNode;
  title: string;
  children: React.ReactNode;
}) {
  return (
    <section className="paper-card space-y-3 p-5">
      <div className="flex items-center gap-2">
        <span className="text-teal">{icon}</span>
        <h3 className="display text-[18px] text-ink">{title}</h3>
      </div>
      {children}
    </section>
  );
}

function Detail({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <span className="text-muted">{label}：</span>
      <span className="text-ink-soft">{value || "未设定"}</span>
    </div>
  );
}



function EdgeEditor({
  edge,
  open,
  saving,
  onOpenChange,
  onSave,
  onDelete,
}: {
  edge: NarrativeLogicEdge | null;
  open: boolean;
  saving: boolean;
  onOpenChange: (open: boolean) => void;
  onSave: (edge: NarrativeLogicEdge) => Promise<void>;
  onDelete: (edge: NarrativeLogicEdge) => Promise<void>;
}) {
  const [relation, setRelation] = useState("");
  const [condition, setCondition] = useState("");
  useEffect(() => {
    setRelation(edge?.relation || "导致");
    setCondition(edge?.condition || "");
  }, [edge]);
  if (!edge) return null;
  return (
    <Modal
      open={open}
      onOpenChange={onOpenChange}
      title={edge.id ? "编辑因果边" : "新增因果边"}
      description={`${edge.fromTitle} → ${edge.toTitle}`}
      width="max-w-[620px]"
      footer={
        <>
          {edge.id ? (
            <Button tone="ghost" className="mr-auto text-coral" disabled={saving} onClick={() => void onDelete(edge)}>
              删除此边
            </Button>
          ) : null}
          <Button tone="ghost" onClick={() => onOpenChange(false)}>取消</Button>
          <Button tone="primary" disabled={saving || !relation.trim()} onClick={() => void onSave({ ...edge, relation, condition })}>
            {saving ? "保存中…" : "保存并重新诊断"}
          </Button>
        </>
      }
    >
      <div className="space-y-3">
        <div className="rounded-xl border border-line bg-paper/50 px-4 py-3 text-[11px] text-ink-soft">
          <span className="font-semibold text-ink">{edge.fromTitle}</span>
          <span className="mx-2 text-teal">→</span>
          <span className="font-semibold text-ink">{edge.toTitle}</span>
        </div>
        <Field label="关系类型" hint="导致 / 迫使 / 开启 / 阻止 / 反转 / 收束">
          <Input value={relation} onChange={(event) => setRelation(event.target.value)} />
        </Field>
        <Field label="成立条件" hint="什么前提成立时，这条因果才发生">
          <Textarea rows={3} value={condition} onChange={(event) => setCondition(event.target.value)} />
        </Field>
      </div>
    </Modal>
  );
}

function CoreEditor({
  view,
  open,
  saving,
  onOpenChange,
  onSave,
}: {
  view: NarrativeBlueprintView | null;
  open: boolean;
  saving: boolean;
  onOpenChange: (open: boolean) => void;
  onSave: (blueprint: Partial<NarrativeBlueprint>) => Promise<void>;
}) {
  const [form, setForm] = useState({
    title: "",
    medium: "novel",
    structureStrategy: "adaptive",
    targetUnits: "0",
    coreDramaticQuestion: "",
    narrativePromise: "",
    openingState: "",
    endingState: "",
    themeQuestion: "",
  });
  useEffect(() => {
    if (!view || !open) return;
    const b = view.blueprint;
    setForm({
      title: b.title,
      medium: b.medium,
      structureStrategy: b.structureStrategy,
      targetUnits: String(b.targetUnits ?? 0),
      coreDramaticQuestion: b.coreDramaticQuestion,
      narrativePromise: b.narrativePromise,
      openingState: b.openingState,
      endingState: b.endingState,
      themeQuestion: b.themeQuestion,
    });
  }, [view, open]);
  const set = (key: keyof typeof form, value: string) =>
    setForm((current) => ({ ...current, [key]: value }));
  return (
    <Modal
      open={open}
      onOpenChange={onOpenChange}
      title="编辑核心叙事契约"
      description="这里改的是候选蓝本；确认后不能静默改写，只能建立新分支或重开规划。"
      width="max-w-[760px]"
      footer={
        <>
          <Button tone="ghost" onClick={() => onOpenChange(false)}>取消</Button>
          <Button
            tone="primary"
            disabled={saving}
            onClick={() =>
              void onSave({
                title: form.title,
                medium: form.medium,
                structureStrategy: form.structureStrategy,
                targetUnits: form.structureStrategy === "fixed" ? Number(form.targetUnits) : null,
                coreDramaticQuestion: form.coreDramaticQuestion,
                narrativePromise: form.narrativePromise,
                openingState: form.openingState,
                endingState: form.endingState,
                themeQuestion: form.themeQuestion,
              })
            }
          >
            {saving ? "保存中…" : "保存并重新诊断"}
          </Button>
        </>
      }
    >
      <div className="space-y-3">
        <Field label="蓝本标题"><Input value={form.title} onChange={(e) => set("title", e.target.value)} /></Field>
        <div className="grid grid-cols-3 gap-3">
          <Field label="媒介">
            <select className="focus-ring h-10 w-full rounded-[10px] border border-line-strong bg-paper-raised px-3 text-[12px]" value={form.medium} onChange={(e) => set("medium", e.target.value)}>
              <option value="novel">小说</option>
              <option value="game">游戏</option>
            </select>
          </Field>
          <Field label="编排策略">
            <select className="focus-ring h-10 w-full rounded-[10px] border border-line-strong bg-paper-raised px-3 text-[12px]" value={form.structureStrategy} onChange={(e) => set("structureStrategy", e.target.value)}>
              <option value="adaptive">逻辑链决定体量</option>
              <option value="fixed">固定章节/任务数</option>
            </select>
          </Field>
          <Field label="目标数">
            <Input type="number" min={1} disabled={form.structureStrategy !== "fixed"} value={form.targetUnits} onChange={(e) => set("targetUnits", e.target.value)} />
          </Field>
        </div>
        <Field label="核心戏剧问题"><Textarea rows={2} value={form.coreDramaticQuestion} onChange={(e) => set("coreDramaticQuestion", e.target.value)} /></Field>
        <Field label="持续叙事承诺"><Textarea rows={2} value={form.narrativePromise} onChange={(e) => set("narrativePromise", e.target.value)} /></Field>
        <div className="grid grid-cols-2 gap-3">
          <Field label="开局长板"><Textarea rows={3} value={form.openingState} onChange={(e) => set("openingState", e.target.value)} /></Field>
          <Field label="终局状态"><Textarea rows={3} value={form.endingState} onChange={(e) => set("endingState", e.target.value)} /></Field>
        </div>
        <Field label="主题追问"><Textarea rows={2} value={form.themeQuestion} onChange={(e) => set("themeQuestion", e.target.value)} /></Field>
      </div>
    </Modal>
  );
}

function ElementEditor({
  element,
  open,
  saving,
  onOpenChange,
  onSave,
}: {
  element: NarrativeElement | null;
  open: boolean;
  saving: boolean;
  onOpenChange: (open: boolean) => void;
  onSave: (element: NarrativeElement) => Promise<void>;
}) {
  const [form, setForm] = useState<NarrativeElement | null>(element);
  useEffect(() => setForm(element), [element]);
  if (!form) return null;
  const set = (key: keyof NarrativeElement, value: string | string[]) =>
    setForm((current) => (current ? { ...current, [key]: value } : current));
  return (
    <Modal
      open={open}
      onOpenChange={onOpenChange}
      title={`编辑${form.kind === "character" ? "人物" : "设定"} · ${form.name}`}
      width="max-w-[760px]"
      footer={
        <>
          <Button tone="ghost" onClick={() => onOpenChange(false)}>取消</Button>
          <Button tone="primary" disabled={saving} onClick={() => void onSave(form)}>
            {saving ? "保存中…" : "保存并重新诊断"}
          </Button>
        </>
      }
    >
      <div className="space-y-3">
        <div className="grid grid-cols-[120px_minmax(0,1fr)_160px] gap-3">
          <Field label="类型"><Input value={form.kind} onChange={(e) => set("kind", e.target.value)} /></Field>
          <Field label="名称"><Input value={form.name} onChange={(e) => set("name", e.target.value)} /></Field>
          <Field label="结构功能"><Input value={form.role} onChange={(e) => set("role", e.target.value)} /></Field>
        </div>
        <Field label="可验证摘要"><Textarea rows={3} value={form.summary} onChange={(e) => set("summary", e.target.value)} /></Field>
        <div className="grid grid-cols-2 gap-3">
          <Field label="外部目标"><Textarea rows={2} value={form.externalGoal} onChange={(e) => set("externalGoal", e.target.value)} /></Field>
          <Field label="内在需要"><Textarea rows={2} value={form.internalNeed} onChange={(e) => set("internalNeed", e.target.value)} /></Field>
          <Field label="恐惧"><Textarea rows={2} value={form.fear} onChange={(e) => set("fear", e.target.value)} /></Field>
          <Field label="缺陷"><Textarea rows={2} value={form.flaw} onChange={(e) => set("flaw", e.target.value)} /></Field>
          <Field label="筹码 / 资源"><Textarea rows={2} value={form.leverage} onChange={(e) => set("leverage", e.target.value)} /></Field>
          <Field label="秘密"><Textarea rows={2} value={form.secret} onChange={(e) => set("secret", e.target.value)} /></Field>
        </div>
        <Field label="硬约束" hint="每行一条">
          <Textarea rows={3} value={form.constraints.join("\n")} onChange={(e) => set("constraints", e.target.value.split("\n").map((x) => x.trim()).filter(Boolean))} />
        </Field>
      </div>
    </Modal>
  );
}

function TimelineEditor({
  event,
  open,
  saving,
  onOpenChange,
  onSave,
}: {
  event: NarrativeTimelineEvent | null;
  open: boolean;
  saving: boolean;
  onOpenChange: (open: boolean) => void;
  onSave: (event: NarrativeTimelineEvent) => Promise<void>;
}) {
  const [form, setForm] = useState<NarrativeTimelineEvent | null>(event);
  useEffect(() => setForm(event), [event]);
  if (!form) return null;
  const set = (key: keyof NarrativeTimelineEvent, value: string | string[] | number) =>
    setForm((current) => (current ? { ...current, [key]: value } : current));
  return (
    <Modal
      open={open}
      onOpenChange={onOpenChange}
      title={`编辑时间线 · ${form.title}`}
      width="max-w-[760px]"
      footer={
        <>
          <Button tone="ghost" onClick={() => onOpenChange(false)}>取消</Button>
          <Button tone="primary" disabled={saving} onClick={() => void onSave(form)}>
            {saving ? "保存中…" : "保存并重新诊断"}
          </Button>
        </>
      }
    >
      <div className="space-y-3">
        <div className="grid grid-cols-[80px_minmax(0,1fr)_180px] gap-3">
          <Field label="顺序"><Input type="number" min={1} value={form.orderNo} onChange={(e) => set("orderNo", Number(e.target.value))} /></Field>
          <Field label="事件"><Input value={form.title} onChange={(e) => set("title", e.target.value)} /></Field>
          <Field label="故事时间"><Input value={form.storyTime} onChange={(e) => set("storyTime", e.target.value)} /></Field>
        </div>
        <Field label="前因"><Textarea rows={2} value={form.cause} onChange={(e) => set("cause", e.target.value)} /></Field>
        <Field label="参与者" hint="逗号分隔"><Input value={form.participants.join("、")} onChange={(e) => set("participants", e.target.value.split(/[，,、]/).map((x) => x.trim()).filter(Boolean))} /></Field>
        <div className="grid grid-cols-2 gap-3">
          <Field label="直接结果"><Textarea rows={2} value={form.outcome} onChange={(e) => set("outcome", e.target.value)} /></Field>
          <Field label="长期后果"><Textarea rows={2} value={form.consequence} onChange={(e) => set("consequence", e.target.value)} /></Field>
        </div>
        <Field label="揭示时点"><Input value={form.revealAt} onChange={(e) => set("revealAt", e.target.value)} /></Field>
      </div>
    </Modal>
  );
}

function LogicNodeEditor({
  node,
  open,
  saving,
  onOpenChange,
  onSave,
}: {
  node: NarrativeLogicNode | null;
  open: boolean;
  saving: boolean;
  onOpenChange: (open: boolean) => void;
  onSave: (node: NarrativeLogicNode) => Promise<void>;
}) {
  const [form, setForm] = useState<NarrativeLogicNode | null>(node);
  useEffect(() => setForm(node), [node]);
  if (!form) return null;
  const set = (key: keyof NarrativeLogicNode, value: string | string[] | number) =>
    setForm((current) => (current ? { ...current, [key]: value } : current));
  return (
    <Modal
      open={open}
      onOpenChange={onOpenChange}
      title={`编辑逻辑节点 #${form.orderNo}`}
      description="必须同时成立前因、选择、代价、结果、状态变化和认知变化。"
      width="max-w-[860px]"
      footer={
        <>
          <Button tone="ghost" onClick={() => onOpenChange(false)}>取消</Button>
          <Button tone="primary" disabled={saving} onClick={() => void onSave(form)}>
            {saving ? "保存中…" : "保存并重新诊断"}
          </Button>
        </>
      }
    >
      <div className="space-y-3">
        <div className="grid grid-cols-[80px_minmax(0,1fr)_160px] gap-3">
          <Field label="顺序"><Input type="number" min={1} value={form.orderNo} onChange={(e) => set("orderNo", Number(e.target.value))} /></Field>
          <Field label="节点名"><Input value={form.title} onChange={(e) => set("title", e.target.value)} /></Field>
          <Field label="类型"><Input value={form.kind} onChange={(e) => set("kind", e.target.value)} /></Field>
        </div>
        <div className="grid grid-cols-2 gap-3">
          <Field label="前因"><Textarea rows={2} value={form.cause} onChange={(e) => set("cause", e.target.value)} /></Field>
          <Field label="人物选择"><Textarea rows={2} value={form.choice} onChange={(e) => set("choice", e.target.value)} /></Field>
          <Field label="不可逆代价"><Textarea rows={2} value={form.cost} onChange={(e) => set("cost", e.target.value)} /></Field>
          <Field label="直接结果"><Textarea rows={2} value={form.outcome} onChange={(e) => set("outcome", e.target.value)} /></Field>
          <Field label="状态变化"><Textarea rows={2} value={form.stateChange} onChange={(e) => set("stateChange", e.target.value)} /></Field>
          <Field label="认知变化"><Textarea rows={2} value={form.knowledgeChange} onChange={(e) => set("knowledgeChange", e.target.value)} /></Field>
        </div>
        <div className="grid grid-cols-2 gap-3">
          <Field label="参与者" hint="逗号分隔"><Input value={form.participants.join("、")} onChange={(e) => set("participants", e.target.value.split(/[，,、]/).map((x) => x.trim()).filter(Boolean))} /></Field>
          <Field label="地点"><Input value={form.location} onChange={(e) => set("location", e.target.value)} /></Field>
        </div>
        <Field label="故事时间"><Input value={form.storyTime} onChange={(e) => set("storyTime", e.target.value)} /></Field>
      </div>
    </Modal>
  );
}
