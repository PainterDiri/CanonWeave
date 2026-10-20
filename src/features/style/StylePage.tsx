import { useCallback, useEffect, useMemo, useState } from "react";
import {
  ArrowLeftRight,
  Check,
  LoaderCircle,
  Paintbrush,
  Plus,
  SlidersHorizontal,
} from "lucide-react";
import { Badge, Button, EmptyState, Field, Input, Modal, Textarea } from "@/components/ui";
import {
  ipc,
  type StyleCompare,
  type StyleMetrics,
  type StylePreset,
  type StyleProfile,
  type StyleSample,
  type PreferenceSummary,
} from "@/lib/ipc";

export function StylePage() {
  const [presets, setPresets] = useState<StylePreset[]>([]);
  const [profiles, setProfiles] = useState<StyleProfile[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [draft, setDraft] = useState<StyleProfile | null>(null);
  const [contract, setContract] = useState("");
  const [samples, setSamples] = useState<StyleSample[]>([]);
  const [sampleOpen, setSampleOpen] = useState(false);
  const [sampleText, setSampleText] = useState("");
  const [sampleSource, setSampleSource] = useState("author");
  const [sampleSpan, setSampleSpan] = useState("");
  const [metrics, setMetrics] = useState<StyleMetrics | null>(null);
  const [compareOpen, setCompareOpen] = useState(false);
  const [leftText, setLeftText] = useState("");
  const [rightText, setRightText] = useState("");
  const [comparison, setComparison] = useState<StyleCompare | null>(null);
  const [preferenceSummary, setPreferenceSummary] = useState<PreferenceSummary | null>(null);
  const [preferenceReason, setPreferenceReason] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async (select?: string) => {
    try {
      const [presetList, profileList, preferences] = await Promise.all([
        ipc.stylePresets(),
        ipc.styleProfiles(),
        ipc.preferenceSummary(),
      ]);
      setPresets(presetList);
      setProfiles(profileList);
      setPreferenceSummary(preferences);
      const next =
        profileList.find((profile) => profile.id === (select ?? selectedId)) ??
        profileList[0] ??
        null;
      setSelectedId(next?.id ?? null);
      setDraft(next ? structuredClone(next) : null);
      if (next) {
        setSamples(await ipc.styleSamples(next.id));
        setContract(await ipc.styleContract(next.id));
      } else {
        setSamples([]);
        setContract("");
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, [selectedId]);

  useEffect(() => {
    void load();
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  const applyPreset = async (preset: StylePreset) => {
    setBusy(true);
    try {
      const profile = await ipc.styleApplyPreset(preset.id, preset.name);
      await load(profile.id);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const save = async () => {
    if (!draft) return;
    setBusy(true);
    setError(null);
    try {
      const action = Math.max(0, 100 - draft.ratios.dialogue - draft.ratios.description - draft.ratios.interiority);
      const saved = await ipc.styleProfileSave({
        id: draft.id,
        scope: draft.scope,
        scopeId: draft.scopeId,
        name: draft.name,
        pov: draft.pov,
        person: draft.person,
        tense: draft.tense,
        syntax: {
          ...draft.syntax,
          avgSentenceMin: Math.max(6, draft.syntax.avgSentenceMax - 8),
        },
        lexicon: draft.lexicon,
        imagery: draft.imagery,
        rhetoric: draft.rhetoric,
        emotion: draft.emotion,
        ratios: { ...draft.ratios, action },
        antiStyle: draft.antiStyle,
      });
      setProfiles((prev) => prev.map((p) => (p.id === saved.id ? saved : p)));
      setDraft(structuredClone(saved));
      setContract(await ipc.styleContract(saved.id));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const addSample = async () => {
    if (!draft) return;
    setBusy(true);
    try {
      const sample = await ipc.styleSampleAdd({
        profileId: draft.id,
        text: sampleText,
        sourceType: sampleSource,
        sourceSpan: sampleSpan,
        tags: [],
      });
      setSamples((prev) => [sample, ...prev]);
      setMetrics(await ipc.styleAnalyze(sampleText));
      setSampleOpen(false);
      setSampleText("");
      setSampleSpan("");
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const calibrate = async () => {
    if (!draft) return;
    setBusy(true);
    try {
      const profile = await ipc.styleCalibrate(draft.id);
      setDraft(structuredClone(profile));
      setProfiles((prev) => prev.map((p) => (p.id === profile.id ? profile : p)));
      setContract(await ipc.styleContract(profile.id));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const compare = async () => {
    try {
      setComparison(await ipc.styleCompare(leftText, rightText));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const choosePreference = async (side: "left" | "right") => {
    if (!comparison || !leftText.trim() || !rightText.trim()) return;
    setBusy(true);
    try {
      const stamp = Date.now();
      const leftRunId = `manual-left-${stamp}`;
      const rightRunId = `manual-right-${stamp}`;
      await ipc.preferenceSave({
        sceneId: null,
        leftRunId,
        rightRunId,
        chosenRunId: side === "left" ? leftRunId : rightRunId,
        reason: preferenceReason,
        leftText,
        rightText,
      });
      const summary = await ipc.preferenceSummary();
      setPreferenceSummary(summary);
      setPreferenceReason("");
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const set = <K extends keyof StyleProfile>(key: K, value: StyleProfile[K]) => {
    if (draft) setDraft({ ...draft, [key]: value });
  };

  const derivedDistance = draft ? 100 - draft.emotion.interiority : 50;

  return (
    <div className="grid grid-cols-[220px_minmax(0,1fr)_300px] gap-4">
      <aside className="paper-card flex h-[620px] min-h-0 flex-col overflow-hidden">
        <div className="flex items-center justify-between border-b border-line px-3.5 py-2.5">
          <span className="text-[11px] font-semibold text-ink-soft">文笔档案</span>
          <span className="text-[10px] text-muted">{profiles.length}</span>
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto p-1.5">
          {profiles.map((profile) => (
            <button
              key={profile.id}
              onClick={() => void load(profile.id)}
              className={`mb-1 w-full rounded-lg px-3 py-2.5 text-left transition ${
                profile.id === selectedId ? "bg-teal-soft/70" : "hover:bg-ink/[0.03]"
              }`}
            >
              <div className="text-[11.5px] font-medium text-ink">{profile.name}</div>
              <div className="mt-1 flex items-center gap-1.5 text-[9.5px] text-muted">
                <span>{scopeLabel(profile.scope)}</span>
                <span>· v{profile.version}</span>
              </div>
            </button>
          ))}
        </div>
        <div className="border-t border-line p-2.5">
          <div className="mb-2 text-[9.5px] leading-4 text-muted">从预设建立新档案</div>
          <select
            className="w-full rounded-lg border border-line bg-paper px-2 py-1.5 text-[10.5px] text-ink-soft outline-none"
            value=""
            onChange={(e) => {
              const preset = presets.find((item) => item.id === e.target.value);
              if (preset) void applyPreset(preset);
            }}
          >
            <option value="">选择预设…</option>
            {presets.map((preset) => (
              <option key={preset.id} value={preset.id}>
                {preset.name}
              </option>
            ))}
          </select>
        </div>
      </aside>

      {!draft ? (
        <div className="col-span-2">
          <EmptyState
            icon={<Paintbrush size={20} />}
            title="建立项目文笔"
            description="文笔只回答“这个镜头怎么写”，不改变世界事实。可以从预设开始，再用六维滑杆和作者样本校准。"
            action={
              <div className="grid max-w-2xl grid-cols-4 gap-2">
                {presets.map((preset) => (
                  <button
                    key={preset.id}
                    onClick={() => void applyPreset(preset)}
                    className="rounded-xl border border-line bg-paper-raised p-3 text-left transition hover:border-teal/40"
                  >
                    <div className="text-[11.5px] font-medium text-ink">{preset.name}</div>
                    <div className="mt-1 text-[9.5px] leading-4 text-muted">{preset.description}</div>
                  </button>
                ))}
              </div>
            }
          />
        </div>
      ) : (
        <>
          <section className="paper-card flex h-[620px] min-h-0 flex-col overflow-hidden">
            <div className="flex items-start justify-between border-b border-line px-5 py-4">
              <div>
                <div className="flex items-center gap-2">
                  <input
                    value={draft.name}
                    onChange={(e) => set("name", e.target.value)}
                    className="display border-none bg-transparent text-[19px] text-ink outline-none"
                  />
                  <Badge tone="teal">v{draft.version}</Badge>
                </div>
                <div className="mt-1 text-[10px] text-muted">
                  {scopeLabel(draft.scope)} · 修改会生成新版本，不影响事实版本
                </div>
              </div>
              <div className="flex gap-2">
                <Button size="sm" onClick={() => setCompareOpen(true)} icon={<ArrowLeftRight size={12} />}>
                  A/B 对比
                </Button>
                <Button size="sm" tone="primary" onClick={() => void save()} disabled={busy}>
                  {busy ? <LoaderCircle size={12} className="animate-spin" /> : <Check size={12} />}
                  保存
                </Button>
              </div>
            </div>
            <div className="min-h-0 flex-1 overflow-y-auto px-5 py-4">
              <div className="mb-4 flex items-center gap-1.5 text-[10px] font-medium tracking-wide text-muted">
                <SlidersHorizontal size={11} /> 六维快捷调节
              </div>
              <div className="grid grid-cols-2 gap-x-6 gap-y-4">
                <Slider
                  label="句子长度"
                  value={draft.syntax.avgSentenceMax}
                  min={12}
                  max={40}
                  hint={`${draft.syntax.avgSentenceMin}–${draft.syntax.avgSentenceMax} 字`}
                  onChange={(value) =>
                    set("syntax", {
                      ...draft.syntax,
                      avgSentenceMax: value,
                      avgSentenceMin: Math.max(6, value - 8),
                    })
                  }
                />
                <Slider
                  label="描写密度"
                  value={draft.ratios.description}
                  min={5}
                  max={60}
                  hint={`${draft.ratios.description}%`}
                  onChange={(value) =>
                    set("ratios", { ...draft.ratios, description: value })
                  }
                />
                <Slider
                  label="情绪外显"
                  value={draft.emotion.externality}
                  min={0}
                  max={100}
                  hint={`${draft.emotion.externality}%`}
                  onChange={(value) =>
                    set("emotion", { ...draft.emotion, externality: value, interiority: 100 - value })
                  }
                />
                <Slider
                  label="意象密度"
                  value={draft.imagery.sensoryPer1000}
                  min={0}
                  max={30}
                  hint={`${draft.imagery.sensoryPer1000}/千字`}
                  onChange={(value) =>
                    set("imagery", { ...draft.imagery, sensoryPer1000: value })
                  }
                />
                <Slider
                  label="对话占比"
                  value={draft.ratios.dialogue}
                  min={0}
                  max={60}
                  hint={`${draft.ratios.dialogue}%`}
                  onChange={(value) => set("ratios", { ...draft.ratios, dialogue: value })}
                />
                <Slider
                  label="叙述距离"
                  value={derivedDistance}
                  min={0}
                  max={100}
                  hint={derivedDistance < 35 ? "贴近" : derivedDistance > 65 ? "疏远" : "中距"}
                  onChange={(value) =>
                    set("emotion", { ...draft.emotion, interiority: 100 - value })
                  }
                />
              </div>

              <div className="hairline my-5" />
              <div className="grid grid-cols-3 gap-3">
                <Field label="视角">
                  <Input value={draft.pov} onChange={(e) => set("pov", e.target.value)} />
                </Field>
                <Field label="人称">
                  <Input value={draft.person} onChange={(e) => set("person", e.target.value)} />
                </Field>
                <Field label="时态">
                  <Input value={draft.tense} onChange={(e) => set("tense", e.target.value)} />
                </Field>
              </div>
              <div className="mt-3 grid grid-cols-2 gap-3">
                <Field label="语域">
                  <Input
                    value={draft.lexicon.register}
                    onChange={(e) =>
                      set("lexicon", { ...draft.lexicon, register: e.target.value })
                    }
                  />
                </Field>
                <Field label="母题" hint="逗号分隔">
                  <Input
                    value={draft.imagery.motifs.join("，")}
                    onChange={(e) =>
                      set("imagery", {
                        ...draft.imagery,
                        motifs: e.target.value.split(/[，,]/).map((s) => s.trim()).filter(Boolean),
                      })
                    }
                  />
                </Field>
              </div>
              <div className="mt-3">
                <Field label="禁例" hint="逗号分隔；用于陈词与 AI 腔检查">
                  <Input
                    value={draft.antiStyle.join("，")}
                    onChange={(e) =>
                      set(
                        "antiStyle",
                        e.target.value.split(/[，,]/).map((s) => s.trim()).filter(Boolean),
                      )
                    }
                  />
                </Field>
              </div>

              <div className="mt-5 rounded-xl border border-line bg-paper/60 p-4">
                <div className="mb-2 flex items-center justify-between text-[10px] font-medium tracking-wide text-muted">
                  <span>文笔契约预览</span>
                  <span>进入 L6 · 低于硬事实</span>
                </div>
                <pre className="max-h-[190px] overflow-auto font-mono text-[10px] leading-5 whitespace-pre-wrap text-ink-soft">
                  {contract || "保存后生成契约"}
                </pre>
              </div>
            </div>
          </section>

          <aside className="paper-card flex h-[620px] min-h-0 flex-col overflow-hidden">
            <div className="flex items-center justify-between border-b border-line px-4 py-2.5">
              <span className="text-[11px] font-semibold text-ink-soft">样本锚点</span>
              <button
                onClick={() => setSampleOpen(true)}
                className="flex h-6 w-6 items-center justify-center rounded-md text-muted hover:bg-ink/5 hover:text-teal"
                title="添加样本"
              >
                <Plus size={13} />
              </button>
            </div>
            <div className="min-h-0 flex-1 overflow-y-auto px-3.5 py-3">
              {samples.length === 0 ? (
                <div className="rounded-lg border border-dashed border-line-strong px-3 py-4 text-[10.5px] leading-5 text-muted">
                  贴入 3 段以上作者认可的原文。样本必须带来源，系统不支持按在世作家姓名仿写。
                </div>
              ) : (
                <div className="space-y-2">
                  {samples.map((sample) => (
                    <div key={sample.id} className="rounded-lg border border-line bg-paper/60 p-2.5">
                      <div className="text-[9px] text-muted">
                        {sample.sourceType} · {sample.sourceSpan}
                      </div>
                      <p className="mt-1.5 line-clamp-3 text-[10.5px] leading-5 text-ink-soft">
                        {sample.text}
                      </p>
                    </div>
                  ))}
                </div>
              )}
              {samples.length > 0 ? (
                <Button size="sm" className="mt-3 w-full" onClick={() => void calibrate()} disabled={busy}>
                  用 {samples.length} 段样本校准
                </Button>
              ) : null}

              {metrics ? (
                <div className="mt-4">
                  <div className="mb-2 text-[10px] font-medium tracking-wide text-muted">样本指标</div>
                  <MetricGrid metrics={metrics} />
                </div>
              ) : null}
            </div>
            {error ? (
              <div className="border-t border-line px-3 py-2 text-[9.5px] leading-4 text-coral">
                {error}
              </div>
            ) : null}
          </aside>
        </>
      )}

      <Modal
        open={sampleOpen}
        onOpenChange={setSampleOpen}
        title="添加作者样本"
        description="来源必须可追溯。示例：作者原创 / 已授权 / 公版 / 导入稿件。"
        footer={
          <>
            <Button tone="ghost" onClick={() => setSampleOpen(false)}>
              取消
            </Button>
            <Button
              tone="primary"
              onClick={() => void addSample()}
              disabled={busy || sampleText.trim().length < 40}
            >
              保存样本
            </Button>
          </>
        }
      >
        <div className="space-y-3">
          <div className="grid grid-cols-2 gap-3">
            <Field label="来源类型">
              <select
                value={sampleSource}
                onChange={(e) => setSampleSource(e.target.value)}
                className="h-10 w-full rounded-[10px] border border-line-strong bg-paper-raised px-3 text-[12px]"
              >
                <option value="author">作者原创</option>
                <option value="licensed">已授权文本</option>
                <option value="public_domain">公版文本</option>
                <option value="import">导入稿件</option>
                <option value="chat">对话采纳</option>
              </select>
            </Field>
            <Field label="来源标记">
              <Input
                value={sampleSpan}
                onChange={(e) => setSampleSpan(e.target.value)}
                placeholder="例如：草稿 2026-08 / 第 3 章"
              />
            </Field>
          </div>
          <Field label="样本正文" hint="至少 40 字">
            <Textarea
              rows={8}
              value={sampleText}
              onChange={(e) => setSampleText(e.target.value)}
              placeholder="贴入一段你认可的文字…"
            />
          </Field>
        </div>
      </Modal>

      <Modal
        open={compareOpen}
        onOpenChange={setCompareOpen}
        title="文笔 A/B 对比"
        description="对比两段文字的句法、对话与感官指标。选择结果之后会写入本地偏好。"
        width="max-w-[820px]"
        footer={
          <>
            <Button tone="ghost" onClick={() => setCompareOpen(false)}>
              关闭
            </Button>
            <Button tone="primary" onClick={() => void compare()} disabled={!leftText || !rightText}>
              比较
            </Button>
          </>
        }
      >
        <div className="grid grid-cols-2 gap-3">
          <Field label="版本 A">
            <Textarea rows={7} value={leftText} onChange={(e) => setLeftText(e.target.value)} />
          </Field>
          <Field label="版本 B">
            <Textarea rows={7} value={rightText} onChange={(e) => setRightText(e.target.value)} />
          </Field>
        </div>
        {comparison ? (
          <div className="mt-4 grid grid-cols-2 gap-4">
            <div>
              <div className="mb-2 text-[10px] font-medium text-muted">A</div>
              <MetricGrid metrics={comparison.left} />
            </div>
            <div>
              <div className="mb-2 text-[10px] font-medium text-muted">B</div>
              <MetricGrid metrics={comparison.right} />
            </div>
            <div className="col-span-2 space-y-1.5">
              {comparison.deltas.map((delta) => (
                <div key={delta.dimension} className="flex items-center justify-between text-[10.5px]">
                  <span className="text-muted">{delta.dimension}</span>
                  <span className="text-ink-soft">
                    {delta.current} → {delta.target}（{delta.delta}）
                  </span>
                </div>
              ))}
            </div>
            <div className="col-span-2 rounded-xl border border-teal/20 bg-teal-soft/30 p-3">
              <div className="flex items-center justify-between">
                <div className="text-[10px] leading-5 text-muted">
                  选择只写入本地偏好，用于后续检索与同风险候选排序，不训练或微调模型。
                </div>
                <Badge tone="teal">{preferenceSummary?.choices ?? 0} 次选择</Badge>
              </div>
              <div className="mt-2 flex gap-2">
                <Input
                  value={preferenceReason}
                  onChange={(e) => setPreferenceReason(e.target.value)}
                  placeholder="选择原因（可选）"
                />
                <Button size="sm" onClick={() => void choosePreference("left")} disabled={busy}>
                  选 A 更好
                </Button>
                <Button size="sm" tone="primary" onClick={() => void choosePreference("right")} disabled={busy}>
                  选 B 更好
                </Button>
              </div>
              {preferenceSummary?.target ? (
                <div className="mt-2 text-[9px] text-muted">
                  本地目标：句长 {preferenceSummary.target.avgSentenceLen} · 对话{" "}
                  {preferenceSummary.target.dialogueRatio}% · 感官{" "}
                  {preferenceSummary.target.sensoryPer1000}/千字
                </div>
              ) : null}
            </div>
          </div>
        ) : null}
      </Modal>
    </div>
  );
}

function Slider({
  label,
  value,
  min,
  max,
  hint,
  onChange,
}: {
  label: string;
  value: number;
  min: number;
  max: number;
  hint: string;
  onChange: (value: number) => void;
}) {
  return (
    <div>
      <div className="mb-1 flex items-center justify-between">
        <span className="text-[11px] text-ink-soft">{label}</span>
        <span className="text-[9.5px] text-muted">{hint}</span>
      </div>
      <input
        type="range"
        min={min}
        max={max}
        value={value}
        onChange={(e) => onChange(Number(e.target.value))}
        className="h-1.5 w-full appearance-none rounded-full bg-line accent-teal"
      />
    </div>
  );
}

function MetricGrid({ metrics }: { metrics: StyleMetrics }) {
  const rows = useMemo(
    () => [
      ["平均句长", `${metrics.avgSentenceLen} 字`],
      ["句长方差", `${metrics.sentenceVariance}`],
      ["对话占比", `${metrics.dialogueRatio}%`],
      ["感官密度", `${metrics.sensoryPer1000}/千字`],
      ["副词密度", `${metrics.adverbPer1000}/千字`],
      ["比喻密度", `${metrics.metaphorPer1000}/千字`],
    ],
    [metrics],
  );
  return (
    <div className="space-y-1.5">
      {rows.map(([label, value]) => (
        <div key={label} className="flex items-center justify-between text-[10px]">
          <span className="text-muted">{label}</span>
          <span className="font-medium text-ink-soft">{value}</span>
        </div>
      ))}
      {metrics.clicheHits.length ? (
        <div className="mt-2 rounded-md bg-coral-soft px-2 py-1.5 text-[9.5px] leading-4 text-coral">
          陈词命中：{metrics.clicheHits.join("、")}
        </div>
      ) : null}
      {metrics.tellWithoutShow.length ? (
        <div className="mt-1 rounded-md bg-amber-soft px-2 py-1.5 text-[9.5px] leading-4 text-amber">
          抽象情绪 {metrics.tellWithoutShow.length} 处，缺少动作/感官承接
        </div>
      ) : null}
    </div>
  );
}

function scopeLabel(scope: string) {
  return (
    {
      project: "项目",
      chapter: "章节",
      character: "人物",
      scene_type: "场景类型",
      scene: "场景",
    }[scope] ?? scope
  );
}
