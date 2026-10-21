import { useCallback, useEffect, useMemo, useState } from "react";
import {
  Check,
  ChevronDown,
  ChevronRight,
  CircleDot,
  Layers3,
  LoaderCircle,
  Pencil,
  Plus,
  Sparkles,
  Trash2,
  Wand2,
} from "lucide-react";
import {
  Badge,
  Button,
  EmptyState,
  Field,
  Input,
  Modal,
  Textarea,
} from "@/components/ui";
import { ipc, type OutlineNode } from "@/lib/ipc";
import { useAppStore } from "@/stores/ui";

type StructureMode = "chapters" | "volumes";

type VolumeDraft = {
  title: string;
  chapterCount: number;
};

const initialVolumes: VolumeDraft[] = [{ title: "第一卷", chapterCount: 60 }];

export function OutlinePanel({
  hasPremise,
  onSwitchToSeed,
}: {
  hasPremise: boolean;
  onSwitchToSeed: () => void;
}) {
  const [nodes, setNodes] = useState<OutlineNode[]>([]);
  const [loading, setLoading] = useState(true);
  const [generating, setGenerating] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [editing, setEditing] = useState<OutlineNode | null>(null);
  const [plannerOpen, setPlannerOpen] = useState(false);
  const [mode, setMode] = useState<StructureMode>("chapters");
  const [totalChapters, setTotalChapters] = useState(24);
  const [volumes, setVolumes] = useState<VolumeDraft[]>(initialVolumes);
  const [expandedVolumes, setExpandedVolumes] = useState<Set<string>>(new Set());
  const [visibleLimit, setVisibleLimit] = useState(80);
  const refreshProject = useAppStore((s) => s.refreshProject);
  const setNav = useAppStore((s) => s.setNav);

  const load = useCallback(async () => {
    try {
      const list = await ipc.outlineList();
      setNodes(list);
      if (!list.length) setPlannerOpen(true);
      setExpandedVolumes(
        new Set(list.filter((node) => node.level === "volume").slice(0, 1).map((node) => node.id)),
      );
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const chapters = useMemo(
    () => nodes.filter((node) => node.level === "chapter"),
    [nodes],
  );
  const volumeNodes = useMemo(
    () => nodes.filter((node) => node.level === "volume"),
    [nodes],
  );
  const confirmed = chapters.filter((node) => node.status === "confirmed");
  const unconfirmedNodes = nodes.filter((node) => node.status !== "confirmed");
  const totalFromVolumes = volumes.reduce(
    (sum, volume) => sum + (Number.isFinite(volume.chapterCount) ? volume.chapterCount : 0),
    0,
  );
  const requestedTotal = mode === "chapters" ? totalChapters : totalFromVolumes;
  const currentMode = volumeNodes.length ? "volumes" : "chapters";

  const generate = async () => {
    if (!hasPremise) {
      onSwitchToSeed();
      return;
    }
    if (mode === "volumes" && volumes.some((volume) => volume.chapterCount < 1)) {
      setError("每一卷至少要有 1 章");
      return;
    }
    setGenerating(true);
    setError(null);
    try {
      const result = await ipc.outlineGenerate({
        mode,
        totalChapters: mode === "chapters" ? totalChapters : null,
        volumes:
          mode === "volumes"
            ? volumes.map((volume) => ({
                title: volume.title.trim() || null,
                chapterCount: volume.chapterCount,
              }))
            : [],
      });
      setNodes(result);
      setPlannerOpen(false);
      await refreshProject();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setGenerating(false);
    }
  };

  const confirm = async (ids: string[]) => {
    try {
      await ipc.outlineConfirm(ids);
      await load();
      await refreshProject();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    }
  };

  const drop = async (id: string) => {
    try {
      await ipc.outlineDrop(id);
      await load();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    }
  };

  const toggleVolume = (id: string) => {
    setExpandedVolumes((current) => {
      const next = new Set(current);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  const updateVolume = (index: number, patch: Partial<VolumeDraft>) => {
    setVolumes((current) =>
      current.map((volume, currentIndex) =>
        currentIndex === index ? { ...volume, ...patch } : volume,
      ),
    );
  };

  if (loading) {
    return (
      <div className="flex h-64 items-center justify-center text-[12px] text-muted">
        <LoaderCircle size={15} className="mr-2 animate-spin" /> 正在读取卷章骨架
      </div>
    );
  }

  return (
    <div className="space-y-4">
      {error ? (
        <div className="rounded-lg border border-coral/25 bg-coral-soft/60 px-4 py-2.5 text-[12px] leading-5 text-coral">
          {error}
        </div>
      ) : null}

      {plannerOpen || nodes.length === 0 ? (
        <StructurePlanner
          hasPremise={hasPremise}
          mode={mode}
          onMode={setMode}
          totalChapters={totalChapters}
          onTotal={setTotalChapters}
          volumes={volumes}
          onVolumes={setVolumes}
          onUpdateVolume={updateVolume}
          requestedTotal={requestedTotal}
          generating={generating}
          onGenerate={() => void generate()}
          onNeedPremise={onSwitchToSeed}
          onCancel={nodes.length ? () => setPlannerOpen(false) : undefined}
        />
      ) : (
        <div className="flex flex-wrap items-center justify-between gap-3 rounded-xl border border-line bg-paper-raised px-4 py-3">
          <div className="flex flex-wrap items-center gap-4 text-[11.5px]">
            <span className="text-muted">
              已确认 <b className="text-ink">{confirmed.length}</b> / {chapters.length} 章
            </span>
            {currentMode === "volumes" ? (
              <span className="text-muted">
                {volumeNodes.length} 卷 ·{" "}
                {volumeNodes.reduce(
                  (sum, volume) =>
                    sum + chapters.filter((chapter) => chapter.parentId === volume.id).length,
                  0,
                )}{" "}
                章
              </span>
            ) : null}
            <span className="text-muted">
              {confirmed.length ? "可以继续写作，也可以补充后续骨架" : "尚未确认任何章节"}
            </span>
          </div>
          <div className="flex gap-2">
            {!confirmed.length ? (
              <Button size="sm" onClick={() => setPlannerOpen(true)} icon={<Wand2 size={12} />}>
                重建候选骨架
              </Button>
            ) : null}
            {unconfirmedNodes.length ? (
              <Button
                size="sm"
                tone="primary"
                onClick={() => void confirm(unconfirmedNodes.map((node) => node.id))}
                icon={<Check size={12} />}
              >
                确认全部
              </Button>
            ) : (
              <Badge tone="teal">骨架已全部确认</Badge>
            )}
          </div>
        </div>
      )}

      {nodes.length > 0 ? (
        currentMode === "volumes" ? (
          <div className="space-y-3">
            {volumeNodes.map((volume, index) => {
              const children = chapters.filter((chapter) => chapter.parentId === volume.id);
              const expanded = expandedVolumes.has(volume.id);
              return (
                <section key={volume.id} className="paper-card overflow-hidden">
                  <div className="flex items-start gap-3 border-b border-line px-5 py-4">
                    <button
                      onClick={() => toggleVolume(volume.id)}
                      className="focus-ring mt-0.5 flex h-7 w-7 shrink-0 items-center justify-center rounded-md text-muted hover:bg-ink/5 hover:text-ink"
                      aria-label={expanded ? "收起本卷" : "展开本卷"}
                    >
                      {expanded ? <ChevronDown size={15} /> : <ChevronRight size={15} />}
                    </button>
                    <div className="min-w-0 flex-1">
                      <div className="flex flex-wrap items-center gap-2">
                        <span className="text-[10px] font-semibold tracking-[0.14em] text-teal">
                          卷 {index + 1}
                        </span>
                        <h3 className="display text-[19px] text-ink">{volume.title}</h3>
                        <Badge tone={volume.status === "confirmed" ? "teal" : "amber"}>
                          {volume.status === "confirmed" ? "已确认" : "候选"}
                        </Badge>
                        <span className="text-[10px] text-muted">{children.length} 章</span>
                      </div>
                      <p className="mt-1 text-[11px] leading-5 text-muted">
                        {volume.goal || "卷目标未设定"} · {volume.turn || "卷转折未设定"}
                      </p>
                    </div>
                    <NodeActions
                      node={volume}
                      onEdit={() => setEditing(volume)}
                      onConfirm={() => void confirm([volume.id])}
                      onDrop={() => void drop(volume.id)}
                    />
                  </div>
                  {expanded ? (
                    <div className="divide-y divide-line">
                      {children.map((chapter) => (
                        <OutlineRow
                          key={chapter.id}
                          node={chapter}
                          compact
                          onEdit={() => setEditing(chapter)}
                          onConfirm={() => void confirm([chapter.id])}
                          onDrop={() => void drop(chapter.id)}
                        />
                      ))}
                    </div>
                  ) : null}
                </section>
              );
            })}
          </div>
        ) : (
          <div className="space-y-2">
            {chapters.slice(0, visibleLimit).map((node) => (
              <OutlineRow
                key={node.id}
                node={node}
                onEdit={() => setEditing(node)}
                onConfirm={() => void confirm([node.id])}
                onDrop={() => void drop(node.id)}
              />
            ))}
            {chapters.length > visibleLimit ? (
              <div className="flex justify-center pt-2">
                <Button size="sm" onClick={() => setVisibleLimit((value) => value + 80)}>
                  继续显示章节
                </Button>
              </div>
            ) : null}
          </div>
        )
      ) : null}

      {confirmed.length > 0 ? (
        <div className="flex items-center justify-between rounded-xl border border-teal/20 bg-teal-soft/40 px-4 py-3">
          <div className="flex items-center gap-2 text-[11.5px] text-teal">
            <ChevronRight size={14} />
            已确认章节可以进入正文。章节内部是否再拆分场次由作者决定。
          </div>
          <Button size="sm" tone="primary" onClick={() => setNav("studio")}>
            进入正文
          </Button>
        </div>
      ) : null}

      <OutlineEditor
        node={editing}
        open={Boolean(editing)}
        onOpenChange={(open) => !open && setEditing(null)}
        onSaved={async () => {
          setEditing(null);
          await load();
        }}
      />
    </div>
  );
}

function StructurePlanner({
  hasPremise,
  mode,
  onMode,
  totalChapters,
  onTotal,
  volumes,
  onVolumes,
  onUpdateVolume,
  requestedTotal,
  generating,
  onGenerate,
  onNeedPremise,
  onCancel,
}: {
  hasPremise: boolean;
  mode: StructureMode;
  onMode: (mode: StructureMode) => void;
  totalChapters: number;
  onTotal: (value: number) => void;
  volumes: VolumeDraft[];
  onVolumes: (volumes: VolumeDraft[]) => void;
  onUpdateVolume: (index: number, patch: Partial<VolumeDraft>) => void;
  requestedTotal: number;
  generating: boolean;
  onGenerate: () => void;
  onNeedPremise: () => void;
  onCancel?: () => void;
}) {
  if (!hasPremise) {
    return (
      <EmptyState
        icon={<Wand2 size={20} />}
        title="先确认故事种子"
        description="卷章规划必须基于作者确认的核心冲突、视角和边界。"
        action={
          <Button tone="primary" onClick={onNeedPremise}>
            返回种子卡
          </Button>
        }
      />
    );
  }
  return (
    <section className="paper-card p-6">
      <div className="flex items-start justify-between gap-5">
        <div>
          <div className="flex items-center gap-2">
            <Layers3 size={16} className="text-teal" />
            <h2 className="display text-[22px] text-ink">先定义作品的叙事长度</h2>
          </div>
          <p className="mt-1.5 max-w-2xl text-[11.5px] leading-6 text-muted">
            作者决定只写若干章，或按卷组织数百章。长篇会分批调用模型生成，避免一次输出截断。
          </p>
        </div>
        {onCancel ? (
          <Button size="sm" tone="ghost" onClick={onCancel}>
            取消
          </Button>
        ) : null}
      </div>

      <div className="mt-5 inline-flex rounded-full border border-line bg-paper p-0.5">
        <button
          onClick={() => onMode("chapters")}
          className={`rounded-full px-3.5 py-1.5 text-[11px] transition ${
            mode === "chapters" ? "bg-teal text-paper-raised" : "text-muted hover:text-ink"
          }`}
        >
          单卷直排
        </button>
        <button
          onClick={() => onMode("volumes")}
          className={`rounded-full px-3.5 py-1.5 text-[11px] transition ${
            mode === "volumes" ? "bg-teal text-paper-raised" : "text-muted hover:text-ink"
          }`}
        >
          分卷结构
        </button>
      </div>

      {mode === "chapters" ? (
        <div className="mt-5 max-w-[320px]">
          <Field label="章节总数" hint="1 至 1200 章">
            <Input
              type="number"
              min={1}
              max={1200}
              value={totalChapters}
              onChange={(event) => onTotal(Number(event.target.value))}
            />
          </Field>
        </div>
      ) : (
        <div className="mt-5 space-y-2">
          {volumes.map((volume, index) => (
            <div
              key={index}
              className="grid grid-cols-[minmax(0,1fr)_150px_36px] items-end gap-2"
            >
              <Field label={`第 ${index + 1} 卷名称`} hint="可留空由 AI 命名">
                <Input
                  value={volume.title}
                  onChange={(event) => onUpdateVolume(index, { title: event.target.value })}
                />
              </Field>
              <Field label="章节数">
                <Input
                  type="number"
                  min={1}
                  max={1200}
                  value={volume.chapterCount}
                  onChange={(event) =>
                    onUpdateVolume(index, { chapterCount: Number(event.target.value) })
                  }
                />
              </Field>
              <button
                onClick={() => onVolumes(volumes.filter((_, itemIndex) => itemIndex !== index))}
                disabled={volumes.length === 1}
                className="focus-ring mb-1 flex h-9 w-9 items-center justify-center rounded-lg text-muted hover:bg-coral-soft hover:text-coral disabled:opacity-30"
                aria-label={`删除第 ${index + 1} 卷`}
              >
                <Trash2 size={13} />
              </button>
            </div>
          ))}
          <button
            onClick={() =>
              onVolumes([
                ...volumes,
                { title: `第${volumes.length + 1}卷`, chapterCount: 60 },
              ])
            }
            className="focus-ring mt-1 flex items-center gap-1.5 rounded-md px-1 py-1 text-[10.5px] text-teal hover:underline"
          >
            <Plus size={11} /> 增加一卷
          </button>
        </div>
      )}

      <div className="mt-5 flex flex-wrap items-center justify-between gap-3 border-t border-line pt-4">
        <div className="text-[10.5px] text-muted">
          全书共 <b className="text-ink">{requestedTotal}</b> 章
          {requestedTotal > 120 ? " · 将分批生成，模型调用次数和耗时较高" : ""}
        </div>
        <Button
          tone="primary"
          onClick={onGenerate}
          disabled={generating || requestedTotal < 1 || requestedTotal > 1200}
          icon={
            generating ? (
              <LoaderCircle size={14} className="animate-spin" />
            ) : (
              <Sparkles size={14} />
            )
          }
        >
          {generating ? "正在逐批生成卷章骨架…" : "生成卷章骨架"}
        </Button>
      </div>
    </section>
  );
}

function OutlineRow({
  node,
  compact = false,
  onEdit,
  onConfirm,
  onDrop,
}: {
  node: OutlineNode;
  compact?: boolean;
  onEdit: () => void;
  onConfirm: () => void;
  onDrop: () => void;
}) {
  return (
    <article className={`group flex gap-4 px-5 ${compact ? "py-3.5" : "paper-card py-4"}`}>
      <div className="flex flex-col items-center">
        <div
          className={`flex h-8 w-8 items-center justify-center rounded-full border text-[10.5px] font-semibold tabular-nums ${
            node.status === "confirmed"
              ? "border-teal bg-teal text-paper-raised"
              : "border-line-strong bg-paper text-ink-soft"
          }`}
        >
          {node.orderNo}
        </div>
        {!compact ? <div className="mt-1 h-full w-px bg-line" /> : null}
      </div>
      <div className="min-w-0 flex-1">
        <div className="flex items-start justify-between gap-3">
          <div className="flex flex-wrap items-center gap-2">
            <h3 className="display text-[16px] text-ink">{node.title}</h3>
            <Badge tone={node.status === "confirmed" ? "teal" : "amber"}>
              {node.status === "confirmed" ? "已确认" : "候选"}
            </Badge>
            <span className="text-[9px] text-muted">
              {node.source === "ai" ? "AI 骨架" : "作者编辑"}
            </span>
          </div>
          <NodeActions
            node={node}
            onEdit={onEdit}
            onConfirm={onConfirm}
            onDrop={onDrop}
          />
        </div>
        <div className={`mt-2 grid grid-cols-4 ${compact ? "gap-3" : "gap-4"}`}>
          {[
            ["目标", node.goal],
            ["冲突", node.conflict],
            ["转折", node.turn],
            ["伏笔", node.payoff],
          ].map(([label, value]) => (
            <div key={label}>
              <div className="mb-1 flex items-center gap-1 text-[9px] text-muted">
                <CircleDot size={8} className="text-teal/50" />
                {label}
              </div>
              <div className="line-clamp-3 text-[10.5px] leading-5 text-ink-soft">
                {value || <span className="text-muted/60">未设定</span>}
              </div>
            </div>
          ))}
        </div>
      </div>
    </article>
  );
}

function NodeActions({
  node,
  onEdit,
  onConfirm,
  onDrop,
}: {
  node: OutlineNode;
  onEdit: () => void;
  onConfirm: () => void;
  onDrop: () => void;
}) {
  return (
    <div className="flex shrink-0 items-center gap-1">
      <button
        onClick={onEdit}
        className="focus-ring flex h-6 w-6 items-center justify-center rounded-md text-muted hover:bg-ink/5 hover:text-teal"
        title="编辑"
      >
        <Pencil size={12} />
      </button>
      {node.status !== "confirmed" ? (
        <button
          onClick={onConfirm}
          className="focus-ring flex h-6 w-6 items-center justify-center rounded-md text-muted hover:bg-teal-soft hover:text-teal"
          title={node.level === "volume" ? "确认本卷及章节" : "确认"}
        >
          <Check size={12} />
        </button>
      ) : null}
      {node.status !== "confirmed" ? (
        <button
          onClick={onDrop}
          className="focus-ring flex h-6 w-6 items-center justify-center rounded-md text-muted hover:bg-coral-soft hover:text-coral"
          title="丢弃"
        >
          <Trash2 size={12} />
        </button>
      ) : null}
    </div>
  );
}

function OutlineEditor({
  node,
  open,
  onOpenChange,
  onSaved,
}: {
  node: OutlineNode | null;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onSaved: () => Promise<void>;
}) {
  const [title, setTitle] = useState("");
  const [goal, setGoal] = useState("");
  const [conflict, setConflict] = useState("");
  const [turn, setTurn] = useState("");
  const [payoff, setPayoff] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (node) {
      setTitle(node.title);
      setGoal(node.goal);
      setConflict(node.conflict);
      setTurn(node.turn);
      setPayoff(node.payoff);
      setError(null);
    }
  }, [node]);

  const save = async () => {
    if (!node) return;
    setBusy(true);
    try {
      await ipc.outlineUpdate({ id: node.id, title, goal, conflict, turn, payoff });
      await onSaved();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setBusy(false);
    }
  };

  const isVolume = node?.level === "volume";
  const unit = isVolume ? "卷" : "章";
  return (
    <Modal
      open={open}
      onOpenChange={onOpenChange}
      title={`编辑第 ${node?.orderNo ?? ""} ${unit}`}
      description={`${unit}级骨架是路线图，不是正文。修改后仍需要单独确认。`}
      footer={
        <>
          <Button tone="ghost" onClick={() => onOpenChange(false)}>
            取消
          </Button>
          <Button tone="primary" onClick={() => void save()} disabled={busy}>
            保存
          </Button>
        </>
      }
    >
      <div className="space-y-3">
        <Field label={`${unit}名`}>
          <Input value={title} onChange={(event) => setTitle(event.target.value)} />
        </Field>
        <Field label="目标" hint={isVolume ? "这一卷要完成什么" : "这一章要改变什么"}>
          <Textarea rows={2} value={goal} onChange={(event) => setGoal(event.target.value)} />
        </Field>
        <Field label="冲突" hint="谁阻止，代价是什么">
          <Textarea
            rows={2}
            value={conflict}
            onChange={(event) => setConflict(event.target.value)}
          />
        </Field>
        <Field label="转折" hint="读者预期如何被打破">
          <Textarea rows={2} value={turn} onChange={(event) => setTurn(event.target.value)} />
        </Field>
        <Field label="伏笔" hint="回收或新增哪条伏笔">
          <Textarea rows={2} value={payoff} onChange={(event) => setPayoff(event.target.value)} />
        </Field>
        {error ? <div className="text-[11px] text-coral">{error}</div> : null}
      </div>
    </Modal>
  );
}
