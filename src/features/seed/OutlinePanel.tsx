import { useCallback, useEffect, useState } from "react";
import {
  Check,
  ChevronRight,
  CircleDot,
  LoaderCircle,
  Pencil,
  Sparkles,
  Trash2,
  Wand2,
} from "lucide-react";
import { Badge, Button, EmptyState, Field, Input, Modal, Textarea } from "@/components/ui";
import { ipc, type OutlineNode } from "@/lib/ipc";
import { useAppStore } from "@/stores/ui";

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
  const refreshProject = useAppStore((s) => s.refreshProject);
  const setNav = useAppStore((s) => s.setNav);

  const load = useCallback(async () => {
    try {
      setNodes(await ipc.outlineList());
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const generate = async () => {
    setGenerating(true);
    setError(null);
    try {
      const result = await ipc.outlineGenerate();
      setNodes((prev) => [
        ...prev.filter((node) => !(node.source === "ai" && node.status === "candidate")),
        ...result,
      ]);
      await refreshProject();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setGenerating(false);
    }
  };

  const confirm = async (ids: string[]) => {
    try {
      await ipc.outlineConfirm(ids);
      await load();
      await refreshProject();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const drop = async (id: string) => {
    try {
      await ipc.outlineDrop(id);
      await load();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const confirmed = nodes.filter((node) => node.status === "confirmed");
  const unconfirmed = nodes.filter((node) => node.status !== "confirmed");

  if (loading) {
    return (
      <div className="flex h-64 items-center justify-center text-[12px] text-muted">
        <LoaderCircle size={15} className="mr-2 animate-spin" /> 正在读取骨架
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

      {nodes.length === 0 ? (
        <EmptyState
          icon={<Wand2 size={20} />}
          title="生成十章骨架"
          description="每章会记录目标、冲突、转折和伏笔操作。首版只生成到章节级，前三章确认后就可以开始第一场。"
          action={
            hasPremise ? (
              <Button
                tone="primary"
                onClick={() => void generate()}
                disabled={generating}
                icon={
                  generating ? (
                    <LoaderCircle size={14} className="animate-spin" />
                  ) : (
                    <Sparkles size={14} />
                  )
                }
              >
                {generating ? "正在推演结构…" : "生成十章骨架"}
              </Button>
            ) : (
              <Button tone="primary" onClick={onSwitchToSeed}>
                先确认种子卡
              </Button>
            )
          }
        />
      ) : (
        <>
          <div className="flex items-center justify-between rounded-xl border border-line bg-paper-raised px-4 py-3">
            <div className="flex items-center gap-4 text-[11.5px]">
              <span className="text-muted">
                已确认 <b className="text-ink">{confirmed.length}</b> / {nodes.length} 章
              </span>
              <span className="text-muted">
                前 3 章 {confirmed.filter((n) => n.orderNo <= 3).length >= 3 ? "已就绪" : "尚未齐备"}
              </span>
            </div>
            <div className="flex gap-2">
              {unconfirmed.length > 0 ? (
                <>
                  <Button
                    size="sm"
                    onClick={() =>
                      void confirm(
                        unconfirmed
                          .filter((n) => n.orderNo <= 3)
                          .slice(0, 3)
                          .map((n) => n.id),
                      )
                    }
                  >
                    确认前 3 章
                  </Button>
                  <Button
                    size="sm"
                    tone="primary"
                    onClick={() => void confirm(unconfirmed.map((n) => n.id))}
                    icon={<Check size={12} />}
                  >
                    全部确认
                  </Button>
                </>
              ) : (
                <Badge tone="teal">骨架已全部确认</Badge>
              )}
            </div>
          </div>

          <div className="space-y-2">
            {nodes.map((node) => (
              <article
                key={node.id}
                className={`paper-card group flex gap-4 px-5 py-4 transition ${
                  node.status === "confirmed" ? "border-teal/20" : ""
                }`}
              >
                <div className="flex flex-col items-center">
                  <div
                    className={`flex h-8 w-8 items-center justify-center rounded-full border text-[11px] font-semibold ${
                      node.status === "confirmed"
                        ? "border-teal bg-teal text-paper-raised"
                        : "border-line-strong bg-paper text-ink-soft"
                    }`}
                  >
                    {node.orderNo}
                  </div>
                  <div className="mt-1 h-full w-px bg-line" />
                </div>
                <div className="min-w-0 flex-1">
                  <div className="flex items-start justify-between gap-3">
                    <div className="flex items-center gap-2">
                      <h3 className="display text-[17px] text-ink">{node.title}</h3>
                      {node.status === "confirmed" ? (
                        <Badge tone="teal">已确认</Badge>
                      ) : (
                        <Badge tone="amber">候选</Badge>
                      )}
                      <span className="text-[9.5px] text-muted">
                        {node.source === "ai" ? "AI 骨架" : "作者编辑"}
                      </span>
                    </div>
                    <div className="flex items-center gap-1 opacity-0 transition group-hover:opacity-100">
                      <button
                        onClick={() => setEditing(node)}
                        className="focus-ring flex h-6 w-6 items-center justify-center rounded-md text-muted hover:bg-ink/5 hover:text-teal"
                        title="编辑"
                      >
                        <Pencil size={12} />
                      </button>
                      {node.status !== "confirmed" ? (
                        <button
                          onClick={() => void confirm([node.id])}
                          className="focus-ring flex h-6 w-6 items-center justify-center rounded-md text-muted hover:bg-teal-soft hover:text-teal"
                          title="确认"
                        >
                          <Check size={12} />
                        </button>
                      ) : null}
                      <button
                        onClick={() => void drop(node.id)}
                        className="focus-ring flex h-6 w-6 items-center justify-center rounded-md text-muted hover:bg-coral-soft hover:text-coral"
                        title="丢弃"
                      >
                        <Trash2 size={12} />
                      </button>
                    </div>
                  </div>
                  <div className="mt-2 grid grid-cols-4 gap-4">
                    {[
                      ["目标", node.goal],
                      ["冲突", node.conflict],
                      ["转折", node.turn],
                      ["伏笔", node.payoff],
                    ].map(([label, value]) => (
                      <div key={label}>
                        <div className="mb-1 flex items-center gap-1 text-[9.5px] text-muted">
                          <CircleDot size={8} className="text-teal/50" />
                          {label}
                        </div>
                        <div className="text-[11px] leading-5 text-ink-soft">
                          {value || <span className="text-muted/60">未设定</span>}
                        </div>
                      </div>
                    ))}
                  </div>
                </div>
              </article>
            ))}
          </div>

          {confirmed.length >= 3 ? (
            <div className="flex items-center justify-between rounded-xl border border-teal/20 bg-teal-soft/40 px-4 py-3">
              <div className="flex items-center gap-2 text-[11.5px] text-teal">
                <ChevronRight size={14} />
                前三章已就绪，可以进入正文编写第一场场景契约。
              </div>
              <Button size="sm" tone="primary" onClick={() => setNav("studio")}>
                进入正文
              </Button>
            </div>
          ) : null}
        </>
      )}

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
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal
      open={open}
      onOpenChange={onOpenChange}
      title={`编辑第 ${node?.orderNo ?? ""} 章`}
      description="骨架是路线图，不是正文。修改后仍需要单独确认。"
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
        <Field label="章名">
          <Input value={title} onChange={(e) => setTitle(e.target.value)} />
        </Field>
        <Field label="目标" hint="这一章要改变什么">
          <Textarea rows={2} value={goal} onChange={(e) => setGoal(e.target.value)} />
        </Field>
        <Field label="冲突" hint="谁阻止，代价是什么">
          <Textarea rows={2} value={conflict} onChange={(e) => setConflict(e.target.value)} />
        </Field>
        <Field label="转折" hint="读者预期如何被打破">
          <Textarea rows={2} value={turn} onChange={(e) => setTurn(e.target.value)} />
        </Field>
        <Field label="伏笔" hint="回收或新增哪条伏笔">
          <Textarea rows={2} value={payoff} onChange={(e) => setPayoff(e.target.value)} />
        </Field>
        {error ? <div className="text-[11px] text-coral">{error}</div> : null}
      </div>
    </Modal>
  );
}
