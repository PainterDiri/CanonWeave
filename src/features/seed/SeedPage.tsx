import { useCallback, useEffect, useState } from "react";
import {
  ArrowRight,
  Check,
  Feather,
  LoaderCircle,
  MessageSquareText,
  Pencil,
  Sparkles,
  Wand2,
} from "lucide-react";
import { Badge, Button, SectionTitle, Textarea } from "@/components/ui";
import { ipc, type Premise } from "@/lib/ipc";
import { useAppStore } from "@/stores/ui";
import { OutlinePanel } from "./OutlinePanel";
import { PremiseEditor } from "./PremiseEditor";

type Tab = "seed" | "outline";

const samples = [
  "一个只能在雨夜出现的旧车站，和一张写着明天日期的车票。",
  "驱魔师发现自己每一次驱魔，都会让别人忘掉被驱魔的人。",
  "父亲葬礼上，所有人都收到了他寄自下周的信。",
];

export function SeedPage() {
  const [tab, setTab] = useState<Tab>("seed");
  const [premises, setPremises] = useState<Premise[]>([]);
  const [active, setActive] = useState<Premise | null>(null);
  const [logline, setLogline] = useState("");
  const [loading, setLoading] = useState(true);
  const [generating, setGenerating] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [editing, setEditing] = useState<Premise | null>(null);
  const [editorOpen, setEditorOpen] = useState(false);
  const refreshProject = useAppStore((s) => s.refreshProject);
  const setNav = useAppStore((s) => s.setNav);

  const load = useCallback(async () => {
    try {
      const [list, current] = await Promise.all([ipc.premiseList(), ipc.premiseActive()]);
      setPremises(list);
      setActive(current);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const generate = async (override?: string) => {
    const text = (override ?? logline).trim();
    if (text.length < 4) {
      setError("请至少写一个具体的画面或冲突");
      return;
    }
    setGenerating(true);
    setError(null);
    try {
      const result = await ipc.seedGenerate(text);
      setPremises((prev) => [...result.directions, ...prev.filter((p) => p.status !== "candidate")]);
      await refreshProject();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setGenerating(false);
    }
  };

  const confirm = async (premise: Premise) => {
    if (!premise.coreConflict.trim()) {
      setEditing(premise);
      setEditorOpen(true);
      setError("这张卡还缺核心冲突，补上后再确认");
      return;
    }
    try {
      const confirmed = await ipc.premiseConfirm(premise.id);
      setActive(confirmed);
      await load();
      await refreshProject();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const candidates = premises.filter((p) => p.status === "candidate");

  return (
    <div className="fade-in mx-auto max-w-[1180px] px-8 py-7">
      <SectionTitle
        eyebrow="Seed · 孵化"
        title="灵感与卷章"
        action={
          active ? (
            <Badge tone="teal">{active.genre || "已确认种子卡"}</Badge>
          ) : (
            <Badge tone="amber">尚未确认种子卡</Badge>
          )
        }
      />

      <div className="mb-5 flex items-center gap-1 border-b border-line">
        {[
          { key: "seed" as const, label: "种子卡", icon: Sparkles },
          { key: "outline" as const, label: "卷章规划", icon: Wand2 },
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
                <span className="absolute inset-x-3 -bottom-px h-0.5 rounded-full bg-teal" />
              ) : null}
            </button>
          );
        })}
      </div>

      {error ? (
        <div className="mb-4 rounded-lg border border-coral/25 bg-coral-soft/60 px-4 py-2.5 text-[12px] leading-5 text-coral">
          {error}
        </div>
      ) : null}

      {loading ? (
        <div className="flex h-64 items-center justify-center text-[12px] text-muted">
          <LoaderCircle size={15} className="mr-2 animate-spin" /> 正在读取种子卡
        </div>
      ) : null}

      {!loading && tab === "seed" ? (
        <div className="space-y-5">
          {active ? <ConfirmedCard premise={active} onEdit={() => { setEditing(active); setEditorOpen(true); }} onOutline={() => setTab("outline")} /> : null}

          {!active ? (
            <section className="paper-card relative overflow-hidden p-7">
              <div
                className="pointer-events-none absolute inset-0 opacity-70"
                style={{
                  backgroundImage:
                    "radial-gradient(circle at 92% 8%, rgba(17,75,69,0.08), transparent 30%), radial-gradient(circle at 4% 100%, rgba(184,134,46,0.08), transparent 32%)",
                }}
              />
              <div className="relative">
                <div className="mb-2 flex items-center gap-2 text-[11px] font-semibold uppercase tracking-[0.18em] text-teal">
                  <Sparkles size={13} /> One line
                </div>
                <h2 className="display text-[26px] text-ink">你的故事从哪一刻开始？</h2>
                <p className="mt-2 max-w-2xl text-[12.5px] leading-6 text-muted">
                  一句话、一个画面，或者一句冲突。系统会给出三种可支撑长篇的走向，每一种都可以继续追问，最终由你确认。
                </p>
                <div className="mt-5">
                  <Textarea
                    rows={4}
                    value={logline}
                    onChange={(e) => setLogline(e.target.value)}
                    placeholder="例如：父亲葬礼上，所有人都收到了他寄自下周的信。"
                    className="bg-paper-raised/90 text-[14px]"
                    onKeyDown={(e) => {
                      if ((e.metaKey || e.ctrlKey) && e.key === "Enter") void generate();
                    }}
                  />
                </div>
                <div className="mt-3 flex flex-wrap items-center justify-between gap-3">
                  <div className="flex flex-wrap gap-2">
                    {samples.map((sample) => (
                      <button
                        key={sample}
                        onClick={() => setLogline(sample)}
                        className="rounded-full border border-line bg-paper-raised/70 px-3 py-1 text-[10.5px] text-muted transition hover:border-teal/40 hover:text-teal"
                      >
                        {sample.slice(0, 18)}…
                      </button>
                    ))}
                  </div>
                  <Button
                    tone="primary"
                    disabled={generating}
                    onClick={() => void generate()}
                    icon={
                      generating ? (
                        <LoaderCircle size={14} className="animate-spin" />
                      ) : (
                        <Wand2 size={14} />
                      )
                    }
                  >
                    {generating ? "正在寻找三种走向…" : "给我三个走向"}
                  </Button>
                </div>
              </div>
            </section>
          ) : null}

          {candidates.length > 0 ? (
            <section>
              <div className="mb-3 flex items-end justify-between">
                <div>
                  <div className="text-[10px] font-semibold uppercase tracking-[0.18em] text-teal">
                    Directions
                  </div>
                  <h2 className="display mt-1 text-[20px] text-ink">
                    {active ? "其他走向（已归档备查）" : "三种走向，选一个继续"}
                  </h2>
                </div>
                <Button size="sm" tone="ghost" onClick={() => void generate()} disabled={generating}>
                  重新生成
                </Button>
              </div>
              <div className="grid grid-cols-3 gap-3">
                {candidates.map((premise, index) => (
                  <DirectionCard
                    key={premise.id}
                    premise={premise}
                    index={index}
                    onConfirm={() => void confirm(premise)}
                    onEdit={() => {
                      setEditing(premise);
                      setEditorOpen(true);
                    }}
                    onAsk={() => setNav("agent")}
                  />
                ))}
              </div>
            </section>
          ) : null}

          {!active && candidates.length === 0 && !generating ? (
            <div className="grid grid-cols-3 gap-3">
              {[
                ["种子卡不是提纲", "它记录的是创作契约：类型、情绪承诺、核心冲突与禁区。"],
                ["AI 推断会被标出", "模型补全的字段标记为 inferred，你可以逐项改成自己的决定。"],
                ["确认后才进入上下文", "确认的种子卡进入任务契约，而不是硬事实层。"],
              ].map(([title, body]) => (
                <div key={title} className="rounded-xl border border-dashed border-line-strong bg-paper/40 px-4 py-4">
                  <div className="text-[12px] font-medium text-ink-soft">{title}</div>
                  <div className="mt-1 text-[11px] leading-5 text-muted">{body}</div>
                </div>
              ))}
            </div>
          ) : null}
        </div>
      ) : null}

      {!loading && tab === "outline" ? (
        <OutlinePanel hasPremise={Boolean(active)} onSwitchToSeed={() => setTab("seed")} />
      ) : null}

      <PremiseEditor
        premise={editing}
        open={editorOpen}
        onOpenChange={setEditorOpen}
        onSaved={async () => {
          setEditorOpen(false);
          await load();
          await refreshProject();
        }}
      />
    </div>
  );
}

function DirectionCard({
  premise,
  index,
  onConfirm,
  onEdit,
  onAsk,
}: {
  premise: Premise;
  index: number;
  onConfirm: () => void;
  onEdit: () => void;
  onAsk: () => void;
}) {
  const inferred = premise.fieldSources?.logline === "inferred";
  return (
    <article className="paper-card flex min-h-[310px] flex-col p-5">
      <div className="mb-3 flex items-center justify-between">
        <div className="flex h-6 w-6 items-center justify-center rounded-full border border-teal/25 text-[10px] font-semibold text-teal">
          {String.fromCharCode(65 + index)}
        </div>
        <div className="flex items-center gap-1.5">
          {inferred ? <Badge tone="amber">AI 推断</Badge> : <Badge tone="teal">作者确认过</Badge>}
        </div>
      </div>
      <h3 className="display text-[17px] leading-7 text-ink">{premise.logline}</h3>
      <div className="mt-3 flex flex-wrap gap-1.5">
        {[premise.genre, premise.subgenre, premise.tone].filter(Boolean).map((tag) => (
          <span key={tag} className="rounded-full bg-ink/5 px-2 py-0.5 text-[10px] text-ink-soft">
            {tag}
          </span>
        ))}
      </div>
      <div className="mt-4 space-y-2 text-[11.5px] leading-5">
        <div>
          <span className="text-muted">核心冲突 · </span>
          <span className="text-ink-soft">{premise.coreConflict || "尚未补全"}</span>
        </div>
        {premise.emotionalPromise ? (
          <div>
            <span className="text-muted">情绪承诺 · </span>
            <span className="text-ink-soft">{premise.emotionalPromise}</span>
          </div>
        ) : null}
      </div>
      <div className="mt-auto pt-5">
        <div className="dashed-rule mb-3" />
        <div className="flex items-center gap-1.5">
          <Button size="sm" tone="primary" onClick={onConfirm} icon={<Check size={12} />}>
            确认这张
          </Button>
          <Button size="sm" tone="ghost" onClick={onEdit} icon={<Pencil size={12} />}>
            编辑
          </Button>
          <Button size="sm" tone="ghost" onClick={onAsk} icon={<MessageSquareText size={12} />}>
            追问
          </Button>
        </div>
      </div>
    </article>
  );
}

function ConfirmedCard({
  premise,
  onEdit,
  onOutline,
}: {
  premise: Premise;
  onEdit: () => void;
  onOutline: () => void;
}) {
  return (
    <section className="paper-card overflow-hidden">
      <div className="flex items-start justify-between gap-6 border-b border-line bg-teal-soft/40 px-6 py-5">
        <div>
          <div className="mb-2 flex items-center gap-2">
            <Badge tone="teal">已确认种子卡</Badge>
            <span className="text-[10px] text-muted">v{premise.version}</span>
          </div>
          <h2 className="display text-[24px] leading-9 text-ink">{premise.logline}</h2>
          <p className="mt-2 max-w-2xl text-[12.5px] leading-6 text-ink-soft">{premise.coreConflict}</p>
        </div>
        <div className="flex shrink-0 gap-2">
          <Button size="sm" onClick={onEdit} icon={<Pencil size={12} />}>
            编辑种子卡
          </Button>
          <Button size="sm" tone="primary" onClick={onOutline} icon={<ArrowRight size={12} />}>
            规划卷章
          </Button>
        </div>
      </div>
      <div className="grid grid-cols-4 gap-0 divide-x divide-line">
        {[
          ["类型", [premise.genre, premise.subgenre].filter(Boolean).join(" · ") || "未定"],
          ["情绪承诺", premise.emotionalPromise || "未定"],
          ["视角", [premise.pov, premise.person, premise.tense].filter(Boolean).join(" · ") || "未定"],
          ["目标篇幅", premise.targetLength || "未定"],
        ].map(([label, value]) => (
          <div key={label} className="px-5 py-4">
            <div className="text-[10px] tracking-wide text-muted">{label}</div>
            <div className="mt-1.5 text-[12px] leading-5 text-ink-soft">{value}</div>
          </div>
        ))}
      </div>
      {(premise.worldHook.length > 0 || premise.taboo.length > 0) && (
        <div className="grid grid-cols-2 gap-5 border-t border-line px-6 py-4">
          <div>
            <div className="mb-1.5 flex items-center gap-1.5 text-[10px] tracking-wide text-muted">
              <Feather size={11} /> 世界钩子
            </div>
            <ul className="space-y-1 text-[11.5px] leading-5 text-ink-soft">
              {premise.worldHook.length ? (
                premise.worldHook.map((item) => <li key={item}>· {item}</li>)
              ) : (
                <li className="text-muted">未设定</li>
              )}
            </ul>
          </div>
          <div>
            <div className="mb-1.5 text-[10px] tracking-wide text-muted">禁区</div>
            <div className="flex flex-wrap gap-1.5">
              {premise.taboo.length ? (
                premise.taboo.map((item) => (
                  <span key={item} className="rounded-full bg-coral-soft px-2 py-0.5 text-[10px] text-coral">
                    {item}
                  </span>
                ))
              ) : (
                <span className="text-[11.5px] text-muted">未设定</span>
              )}
            </div>
          </div>
        </div>
      )}
    </section>
  );
}
