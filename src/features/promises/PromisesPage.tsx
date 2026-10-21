import { useCallback, useEffect, useState } from "react";
import { Feather, LoaderCircle, Plus, ShieldCheck } from "lucide-react";
import { Badge, Button, EmptyState, Field, Input, Modal, SectionTitle, Textarea } from "@/components/ui";
import { AgentSuggestion } from "@/components/AgentSuggestion";
import { ipc, type PayoffCheck, type PromiseRecord } from "@/lib/ipc";
import { useAppStore } from "@/stores/ui";

const stateMeta: Record<string, { label: string; tone: "neutral" | "teal" | "amber" | "coral" }> = {
  planned: { label: "计划中", tone: "neutral" },
  planted: { label: "已埋设", tone: "teal" },
  reinforced: { label: "已推进", tone: "teal" },
  ready: { label: "待兑现", tone: "amber" },
  paid_off: { label: "已兑现", tone: "teal" },
  abandoned: { label: "已放弃", tone: "neutral" },
};

export function PromisesPage() {
  const [promises, setPromises] = useState<PromiseRecord[]>([]);
  const [loading, setLoading] = useState(true);
  const [editing, setEditing] = useState<{ id: string | null; title: string; targetWindow: string; payoffContract: string } | null>(null);
  const [payoff, setPayoff] = useState<PromiseRecord | null>(null);
  const [sourceId, setSourceId] = useState("");
  const [quote, setQuote] = useState("");
  const [stateChange, setStateChange] = useState("");
  const [payoffCheck, setPayoffCheck] = useState<PayoffCheck | null>(null);
  const [error, setError] = useState<string | null>(null);
  const refreshProject = useAppStore((s) => s.refreshProject);

  const load = useCallback(async () => {
    try {
      setPromises(await ipc.promiseList());
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const save = async () => {
    if (!editing) return;
    try {
      await ipc.promiseSave({
        id: editing.id,
        title: editing.title,
        targetWindow: editing.targetWindow,
        payoffContract: editing.payoffContract,
      });
      setEditing(null);
      await load();
      await refreshProject();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const transition = async (promise: PromiseRecord, state: string, evidence = "") => {
    try {
      await ipc.promiseTransition({ id: promise.id, state, evidence });
      setPayoff(null);
      await load();
      await refreshProject();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const openPayoff = (promise: PromiseRecord) => {
    setPayoff(promise);
    setSourceId("");
    setQuote("");
    setStateChange("");
    setPayoffCheck(null);
  };

  const checkPayoff = async () => {
    if (!payoff) return;
    try {
      setPayoffCheck(
        await ipc.promisePayoffCheck({
          id: payoff.id,
          sourceId,
          quote,
          stateChange,
        }),
      );
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const confirmPayoff = async () => {
    if (!payoff) return;
    try {
      await ipc.promisePayoff({
        id: payoff.id,
        sourceId,
        quote,
        stateChange,
      });
      setPayoff(null);
      await load();
      await refreshProject();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  if (loading) {
    return (
      <div className="flex h-64 items-center justify-center text-[12px] text-muted">
        <LoaderCircle size={15} className="mr-2 animate-spin" /> 正在读取伏笔台账
      </div>
    );
  }

  return (
    <div className="fade-in mx-auto max-w-[1120px] px-8 py-7">
      <SectionTitle
        eyebrow="Promises"
        title="伏笔台账"
        action={
          <Button
            tone="primary"
            size="sm"
            onClick={() => setEditing({ id: null, title: "", targetWindow: "", payoffContract: "" })}
            icon={<Plus size={13} />}
          >
            新建伏笔
          </Button>
        }
      />
      <AgentSuggestion outputs={["all", "promises", "timeline"]} />
      {error ? (
        <div className="mb-4 rounded-lg border border-coral/25 bg-coral-soft/60 px-4 py-2.5 text-[12px] text-coral">
          {error}
        </div>
      ) : null}
      {promises.length === 0 ? (
        <EmptyState
          icon={<Feather size={20} />}
          title="还没有埋下伏笔"
          description="每条伏笔都要写清兑现契约：揭示什么信息、改变谁的认知、交付何物或触发哪件事。只出现关键词不算兑现。"
          action={
            <Button
              tone="primary"
              onClick={() => setEditing({ id: null, title: "", targetWindow: "", payoffContract: "" })}
            >
              新建第一条伏笔
            </Button>
          }
        />
      ) : (
        <div className="space-y-2.5">
          {promises.map((promise) => {
            const meta = stateMeta[promise.state] ?? stateMeta.planned;
            return (
              <article key={promise.id} className="paper-card px-5 py-4">
                <div className="flex items-start justify-between gap-4">
                  <div className="flex items-start gap-3">
                    <div className="mt-0.5 flex h-8 w-8 items-center justify-center rounded-full border border-amber/25 bg-amber-soft/50 text-amber">
                      <Feather size={13} />
                    </div>
                    <div>
                      <div className="flex items-center gap-2">
                        <h3 className="display text-[16px] text-ink">{promise.title}</h3>
                        <Badge tone={meta.tone}>{meta.label}</Badge>
                        {promise.targetWindow ? (
                          <span className="text-[10px] text-muted">窗口：{promise.targetWindow}</span>
                        ) : null}
                      </div>
                      <p className="mt-1.5 max-w-3xl text-[11.5px] leading-5 text-ink-soft">
                        {promise.payoffContract || "尚未写清兑现契约"}
                      </p>
                      <div className="mt-2 flex items-center gap-2 text-[9.5px] text-muted">
                        <span>{promise.evidenceIds.length} 条证据</span>
                        {promise.evidenceIds.length ? (
                          <span className="max-w-md truncate">
                            · {promise.evidenceIds.join(" / ")}
                          </span>
                        ) : null}
                      </div>
                    </div>
                  </div>
                  <div className="flex shrink-0 items-center gap-1.5">
                    <Button
                      size="sm"
                      tone="ghost"
                      onClick={() =>
                        setEditing({
                          id: promise.id,
                          title: promise.title,
                          targetWindow: promise.targetWindow,
                          payoffContract: promise.payoffContract,
                        })
                      }
                    >
                      编辑
                    </Button>
                    {promise.state === "planned" ? (
                      <Button size="sm" onClick={() => void transition(promise, "planted")}>
                        标记埋设
                      </Button>
                    ) : null}
                    {promise.state === "planted" ? (
                      <>
                        <Button size="sm" onClick={() => void transition(promise, "reinforced")}>
                          推进
                        </Button>
                        <Button size="sm" onClick={() => void transition(promise, "ready")}>
                          就绪
                        </Button>
                      </>
                    ) : null}
                    {promise.state === "reinforced" ? (
                      <Button size="sm" onClick={() => void transition(promise, "ready")}>
                        标记就绪
                      </Button>
                    ) : null}
                    {promise.state === "ready" ? (
                      <Button
                        size="sm"
                        tone="primary"
                        onClick={() => openPayoff(promise)}
                        icon={<ShieldCheck size={12} />}
                      >
                        兑现
                      </Button>
                    ) : null}
                    {!["paid_off", "abandoned"].includes(promise.state) ? (
                      <Button
                        size="sm"
                        tone="ghost"
                        onClick={() => {
                          if (window.confirm("放弃这条伏笔？它会保留证据，但不再计入未结伏笔。")) {
                            void transition(promise, "abandoned");
                          }
                        }}
                      >
                        放弃
                      </Button>
                    ) : null}
                  </div>
                </div>
              </article>
            );
          })}
        </div>
      )}

      <Modal
        open={Boolean(editing)}
        onOpenChange={(open) => !open && setEditing(null)}
        title={editing?.id ? "编辑伏笔" : "新建伏笔"}
        description="伏笔必须带兑现契约。兑现时还需要原埋设证据、新兑现证据和可定位的状态变化。"
        footer={
          <>
            <Button tone="ghost" onClick={() => setEditing(null)}>
              取消
            </Button>
            <Button tone="primary" onClick={() => void save()} disabled={!editing?.title.trim()}>
              保存
            </Button>
          </>
        }
      >
        {editing ? (
          <div className="space-y-3">
            <Field label="标题">
              <Input
                value={editing.title}
                onChange={(e) => setEditing({ ...editing, title: e.target.value })}
              />
            </Field>
            <Field label="预期窗口" hint="例如：第 3 至 5 章">
              <Input
                value={editing.targetWindow}
                onChange={(e) => setEditing({ ...editing, targetWindow: e.target.value })}
              />
            </Field>
            <Field label="兑现契约" hint="怎样才算真正兑现">
              <Textarea
                rows={4}
                value={editing.payoffContract}
                onChange={(e) => setEditing({ ...editing, payoffContract: e.target.value })}
                placeholder="揭示哪条信息 / 改变谁的认知 / 交付哪件物品 / 触发哪个事件"
              />
            </Field>
          </div>
        ) : null}
      </Modal>

      <Modal
        open={Boolean(payoff)}
        onOpenChange={(open) => !open && setPayoff(null)}
        title="确认伏笔兑现"
        description={`“${payoff?.title ?? ""}”的兑现契约：${payoff?.payoffContract ?? ""}`}
        footer={
          <>
            <Button tone="ghost" onClick={() => setPayoff(null)}>
              取消
            </Button>
            <Button onClick={() => void checkPayoff()} disabled={!sourceId.trim() || !quote.trim()}>
              验证证据
            </Button>
            <Button tone="primary" onClick={() => void confirmPayoff()} disabled={!payoffCheck?.ready}>
              确认兑现
            </Button>
          </>
        }
      >
        <div className="space-y-3">
          <Field label="新证据来源 ID" hint="例如 chapter:ch09 / scene:... / import:...">
            <Input value={sourceId} onChange={(e) => setSourceId(e.target.value)} />
          </Field>
          <Field label="可回查原文片段" hint="至少 8 字；只写关键词不算兑现">
            <Textarea
              rows={3}
              value={quote}
              onChange={(e) => setQuote(e.target.value)}
              placeholder="例如：林澈把钥匙插进抽屉，认出铜齿上刻着父亲的名字。"
            />
          </Field>
          <Field label="读者或故事状态变化" hint="揭示信息 / 改变认知 / 交付物品 / 触发事件">
            <Textarea
              rows={3}
              value={stateChange}
              onChange={(e) => setStateChange(e.target.value)}
              placeholder="例如：林澈改变了对父亲的判断，确认钥匙不是伪造的。"
            />
          </Field>
          {payoffCheck ? (
            <div className={`rounded-lg border px-3 py-2 text-[10px] leading-5 ${
              payoffCheck.ready
                ? "border-teal/20 bg-teal-soft/50 text-teal"
                : "border-amber/30 bg-amber-soft/50 text-amber"
            }`}>
              {payoffCheck.ready
                ? "证据链完整：已有原埋设证据、新定位片段与状态变化。"
                : payoffCheck.blockers.join("；")}
            </div>
          ) : null}
        </div>
      </Modal>
    </div>
  );
}
