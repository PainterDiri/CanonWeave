import { useEffect, useState } from "react";
import { LoaderCircle, Sparkles } from "lucide-react";
import { Button, Field, Input, Modal, Textarea } from "@/components/ui";
import { ipc, type Premise, type PremiseInput } from "@/lib/ipc";

type Form = Omit<PremiseInput, "sourceMessageId" | "authorFields" | "worldHook" | "taboo"> & {
  worldHook: string;
  taboo: string;
};

const empty: Form = {
  logline: "",
  genre: "",
  subgenre: "",
  theme: "",
  emotionalPromise: "",
  tone: "",
  coreConflict: "",
  dramaticQuestion: "",
  antagonisticForce: "",
  worldRule: "",
  endingDirection: "",
  protagonistDesire: "",
  flaw: "",
  secret: "",
  worldHook: "",
  pov: "第三人称限知",
  person: "第三人称",
  tense: "过去时",
  targetLength: "",
  updateRhythm: "",
  endingIntent: "未定",
  taboo: "",
};

function toForm(premise: Premise): Form {
  return {
    logline: premise.logline,
    genre: premise.genre,
    subgenre: premise.subgenre,
    theme: premise.theme,
    emotionalPromise: premise.emotionalPromise,
    tone: premise.tone,
    coreConflict: premise.coreConflict,
    dramaticQuestion: premise.dramaticQuestion,
    antagonisticForce: premise.antagonisticForce,
    worldRule: premise.worldRule,
    endingDirection: premise.endingDirection,
    protagonistDesire: premise.protagonistDesire,
    flaw: premise.flaw,
    secret: premise.secret,
    worldHook: premise.worldHook.join("\n"),
    pov: premise.pov,
    person: premise.person,
    tense: premise.tense,
    targetLength: premise.targetLength,
    updateRhythm: premise.updateRhythm,
    endingIntent: premise.endingIntent,
    taboo: premise.taboo.join("\n"),
  };
}

export function PremiseEditor({
  premise,
  open,
  onOpenChange,
  onSaved,
}: {
  premise: Premise | null;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onSaved: () => Promise<void>;
}) {
  const [form, setForm] = useState<Form>(empty);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (open && premise) {
      setForm(toForm(premise));
      setError(null);
    }
  }, [open, premise]);

  const set = <K extends keyof Form>(key: K, value: Form[K]) =>
    setForm((prev) => ({ ...prev, [key]: value }));

  const save = async () => {
    if (!premise) return;
    setBusy(true);
    setError(null);
    try {
      const initial = toForm(premise);
      const authorFields: string[] = [];
      const keys = Object.keys(initial) as (keyof Form)[];
      for (const key of keys) {
        if (form[key] !== initial[key]) authorFields.push(String(key));
      }
      const input: PremiseInput = {
        ...form,
        worldHook: form.worldHook.split("\n").map((s) => s.trim()).filter(Boolean),
        taboo: form.taboo.split(/[,\n，]/).map((s) => s.trim()).filter(Boolean),
        sourceMessageId: premise.sourceMessageId,
        authorFields,
      };
      await ipc.premiseUpdate(premise.id, input);
      await onSaved();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const inferred = (field: string) => premise?.fieldSources?.[field] === "inferred";

  return (
    <Modal
      open={open}
      onOpenChange={onOpenChange}
      title="编辑种子卡"
      description="种子卡是创作契约，不是世界事实。带“AI 推断”标记的字段还没有经过你的确认。"
      width="max-w-[760px]"
      footer={
        <>
          <Button tone="ghost" onClick={() => onOpenChange(false)}>
            取消
          </Button>
          <Button tone="primary" onClick={() => void save()} disabled={busy}>
            {busy ? <LoaderCircle size={14} className="animate-spin" /> : null}
            保存种子卡
          </Button>
        </>
      }
    >
      <div className="space-y-4">
        <div>
          <div className="mb-1.5 flex items-center justify-between">
            <span className="text-[12px] font-medium text-ink-soft">一句话故事</span>
            {inferred("logline") ? <InferredTag /> : null}
          </div>
          <Textarea
            rows={2}
            value={form.logline}
            onChange={(e) => set("logline", e.target.value)}
            placeholder="谁在什么处境中，想要什么，遇到什么阻力"
          />
        </div>
        <div>
          <div className="mb-1.5 flex items-center justify-between">
            <span className="text-[12px] font-medium text-ink-soft">核心冲突</span>
            {inferred("coreConflict") ? <InferredTag /> : null}
          </div>
          <Textarea
            rows={2}
            value={form.coreConflict}
            onChange={(e) => set("coreConflict", e.target.value)}
            placeholder="谁想要什么、什么在阻止、代价是什么"
          />
        </div>
        <div className="grid grid-cols-2 gap-3">
          <Field label="核心戏剧问题" hint="全书用一个代价选择回答">
            <Textarea
              rows={2}
              value={form.dramaticQuestion}
              onChange={(e) => set("dramaticQuestion", e.target.value)}
            />
          </Field>
          <Field label="结构性阻力" hint="谁或什么在阻止，靠什么阻止">
            <Textarea
              rows={2}
              value={form.antagonisticForce}
              onChange={(e) => set("antagonisticForce", e.target.value)}
            />
          </Field>
          <Field label="持续制造代价的世界规则">
            <Textarea
              rows={2}
              value={form.worldRule}
              onChange={(e) => set("worldRule", e.target.value)}
            />
          </Field>
          <Field label="终局改变什么" hint="世界 / 关系 / 自我认知">
            <Textarea
              rows={2}
              value={form.endingDirection}
              onChange={(e) => set("endingDirection", e.target.value)}
            />
          </Field>
        </div>
        <div className="grid grid-cols-3 gap-3">
          <Field label="类型">
            <Input value={form.genre} onChange={(e) => set("genre", e.target.value)} />
          </Field>
          <Field label="子类型">
            <Input value={form.subgenre} onChange={(e) => set("subgenre", e.target.value)} />
          </Field>
          <Field label="情绪基调">
            <Input value={form.tone} onChange={(e) => set("tone", e.target.value)} />
          </Field>
        </div>
        <div className="grid grid-cols-2 gap-3">
          <Field label="核心命题">
            <Input value={form.theme} onChange={(e) => set("theme", e.target.value)} />
          </Field>
          <Field label="情绪承诺">
            <Input
              value={form.emotionalPromise}
              onChange={(e) => set("emotionalPromise", e.target.value)}
            />
          </Field>
        </div>
        <div className="grid grid-cols-3 gap-3">
          <Field label="主角欲望">
            <Input
              value={form.protagonistDesire}
              onChange={(e) => set("protagonistDesire", e.target.value)}
            />
          </Field>
          <Field label="缺陷">
            <Input value={form.flaw} onChange={(e) => set("flaw", e.target.value)} />
          </Field>
          <Field label="秘密">
            <Input value={form.secret} onChange={(e) => set("secret", e.target.value)} />
          </Field>
        </div>
        <div className="grid grid-cols-3 gap-3">
          <Field label="视角">
            <Input value={form.pov} onChange={(e) => set("pov", e.target.value)} />
          </Field>
          <Field label="人称">
            <Input value={form.person} onChange={(e) => set("person", e.target.value)} />
          </Field>
          <Field label="时态">
            <Input value={form.tense} onChange={(e) => set("tense", e.target.value)} />
          </Field>
        </div>
        <div className="grid grid-cols-3 gap-3">
          <Field label="目标篇幅">
            <Input
              value={form.targetLength}
              onChange={(e) => set("targetLength", e.target.value)}
              placeholder="例如 80 万字"
            />
          </Field>
          <Field label="更新节奏">
            <Input
              value={form.updateRhythm}
              onChange={(e) => set("updateRhythm", e.target.value)}
              placeholder="例如 日更 4000 字"
            />
          </Field>
          <Field label="结局意图">
            <Input
              value={form.endingIntent}
              onChange={(e) => set("endingIntent", e.target.value)}
              placeholder="HE / BE / 开放 / 未定"
            />
          </Field>
        </div>
        <div className="grid grid-cols-2 gap-3">
          <Field label="世界钩子" hint="每行一条">
            <Textarea
              rows={3}
              value={form.worldHook}
              onChange={(e) => set("worldHook", e.target.value)}
              placeholder="一条世界规则，或一个不寻常的设定"
            />
          </Field>
          <Field label="禁区" hint="逗号或换行分隔">
            <Textarea
              rows={3}
              value={form.taboo}
              onChange={(e) => set("taboo", e.target.value)}
              placeholder="不写什么、不能出现什么"
            />
          </Field>
        </div>
        {error ? <div className="text-[11.5px] leading-5 text-coral">{error}</div> : null}
      </div>
    </Modal>
  );
}

function InferredTag() {
  return (
    <span className="inline-flex items-center gap-1 rounded-full bg-amber-soft px-2 py-0.5 text-[9.5px] font-medium text-amber">
      <Sparkles size={9} /> AI 推断
    </span>
  );
}
