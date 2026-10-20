import { useCallback, useEffect, useState } from "react";
import { AudioLines, LoaderCircle, Plus, ScanText } from "lucide-react";
import {
  Badge,
  Button,
  Field,
  Input,
  Modal,
  Textarea,
} from "@/components/ui";
import {
  ipc,
  type CharacterVoiceProfile,
  type CharacterVoiceSample,
  type VoiceCheck,
} from "@/lib/ipc";
import { useAppStore } from "@/stores/ui";

export function VoicePanel({ characterId }: { characterId: string }) {
  const [samples, setSamples] = useState<CharacterVoiceSample[]>([]);
  const [profile, setProfile] = useState<CharacterVoiceProfile | null>(null);
  const [check, setCheck] = useState<VoiceCheck | null>(null);
  const [sampleOpen, setSampleOpen] = useState(false);
  const [checkOpen, setCheckOpen] = useState(false);
  const [sampleText, setSampleText] = useState("");
  const [sourceSpan, setSourceSpan] = useState("");
  const [sourceType, setSourceType] = useState("author");
  const [checkText, setCheckText] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const notify = useAppStore((s) => s.notify);

  const load = useCallback(async () => {
    try {
      const [sampleList, voiceProfile] = await Promise.all([
        ipc.voiceSamples(characterId),
        ipc.voiceProfile(characterId),
      ]);
      setSamples(sampleList);
      setProfile(voiceProfile);
      setCheck(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, [characterId]);

  useEffect(() => {
    void load();
  }, [load]);

  const addSample = async () => {
    setBusy(true);
    setError(null);
    try {
      await ipc.voiceSampleAdd({
        characterId,
        text: sampleText,
        sourceSpan,
        sourceType,
      });
      setSampleOpen(false);
      setSampleText("");
      setSourceSpan("");
      await load();
      notify("人物声音样本已加入质心");
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const runCheck = async () => {
    setBusy(true);
    setError(null);
    try {
      setCheck(await ipc.voiceCheck(characterId, checkText));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const setThreshold = async (
    key: "sentenceLen" | "sentenceVariance" | "punctuation" | "pronoun" | "dialogueAction",
    value: number,
  ) => {
    if (!profile) return;
    setBusy(true);
    try {
      const saved = await ipc.voiceThresholdsSave({
        characterId,
        thresholds: { ...profile.thresholds, [key]: value },
      });
      setProfile(saved);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="mt-5 rounded-xl border border-line bg-paper/60 p-4">
      <div className="flex items-start justify-between gap-3">
        <div>
          <div className="flex items-center gap-1.5 text-[10px] font-semibold tracking-wide text-ink-soft">
            <AudioLines size={12} className="text-teal" />
            人物声音指纹
          </div>
          <p className="mt-1 text-[9.5px] leading-4 text-muted">
            只检查该人物台词与 POV 段落；结果为可解释软提示，不自动改写。
          </p>
        </div>
        <div className="flex gap-1.5">
          <Button size="sm" tone="ghost" onClick={() => setSampleOpen(true)} icon={<Plus size={11} />}>
            样本
          </Button>
          <Button
            size="sm"
            onClick={() => setCheckOpen(true)}
            disabled={!profile?.sampleCount}
            icon={<ScanText size={11} />}
          >
            检查片段
          </Button>
        </div>
      </div>
      {error ? (
        <div className="mt-2 rounded-md bg-coral-soft/60 px-2 py-1.5 text-[9.5px] text-coral">
          {error}
        </div>
      ) : null}
      {samples.length === 0 ? (
        <div className="mt-3 rounded-lg border border-dashed border-line-strong px-3 py-3 text-[10px] leading-5 text-muted">
          还没有独立声音样本。建议加入 3 至 8 段作者认可的该人物台词或 POV 文字。
        </div>
      ) : (
        <>
          <div className="mt-3 flex flex-wrap gap-1.5">
            <Badge tone="teal">{samples.length} 段样本</Badge>
            <Badge tone="neutral">v{profile?.version ?? 0}</Badge>
            <span className="text-[9px] text-muted">
              质心：句长 {profile?.centroid.avgSentenceLen ?? 0} · 标点{" "}
              {profile?.centroid.punctuationPer100 ?? 0}/百字 · 代词{" "}
              {profile?.centroid.preferredPronoun || "—"}
            </span>
          </div>
          {profile ? (
            <div className="mt-3 grid grid-cols-5 gap-1.5">
              {[
                ["句长", "sentenceLen", profile.thresholds.sentenceLen],
                ["波动", "sentenceVariance", profile.thresholds.sentenceVariance],
                ["标点", "punctuation", profile.thresholds.punctuation],
                ["代词", "pronoun", profile.thresholds.pronoun],
                ["对白比", "dialogueAction", profile.thresholds.dialogueAction],
              ].map(([label, key, value]) => (
                <label key={String(key)} className="rounded-md border border-line bg-paper-raised p-1.5">
                  <span className="block text-[8px] text-muted">{label}</span>
                  <input
                    type="number"
                    min={1}
                    value={Number(value)}
                    onChange={(e) =>
                      void setThreshold(
                        key as
                          | "sentenceLen"
                          | "sentenceVariance"
                          | "punctuation"
                          | "pronoun"
                          | "dialogueAction",
                        Number(e.target.value),
                      )
                    }
                    className="mt-0.5 w-full bg-transparent text-[10px] text-ink outline-none"
                  />
                </label>
              ))}
            </div>
          ) : null}
        </>
      )}
      {check ? (
        <div className="mt-3 rounded-lg border border-line bg-paper-raised p-3">
          <div className="flex items-center justify-between">
            <span className="text-[10px] font-medium text-ink-soft">漂移结果</span>
            <Badge tone={check.voiceDrift ? "amber" : "teal"}>
              {check.voiceDrift ? `${check.drifts.length} 项` : "接近质心"}
            </Badge>
          </div>
          <div className="mt-2 space-y-1.5">
            {check.drifts.slice(0, 8).map((drift, index) => (
              <div key={`${drift.dimension}-${drift.segmentIndex}-${index}`} className="text-[9px] leading-4">
                <span className={drift.severity === "major" ? "text-coral" : "text-amber"}>
                  {drift.dimension} {drift.delta > 0 ? "+" : ""}
                  {drift.delta}
                </span>
                <span className="text-muted">
                  {" "}
                  目标 {drift.target} · 片段 {drift.start}–{drift.end}
                </span>
                <div className="line-clamp-2 text-ink-soft">{drift.excerpt}</div>
              </div>
            ))}
          </div>
        </div>
      ) : null}

      <Modal
        open={sampleOpen}
        onOpenChange={setSampleOpen}
        title="添加人物声音样本"
        description="样本来源必须可追溯；系统只用它建立该人物的局部声音质心。"
        footer={
          <>
            <Button tone="ghost" onClick={() => setSampleOpen(false)}>
              取消
            </Button>
            <Button
              tone="primary"
              onClick={() => void addSample()}
              disabled={busy || sampleText.trim().length < 20 || !sourceSpan.trim()}
            >
              加入样本
            </Button>
          </>
        }
      >
        <div className="space-y-3">
          <div className="grid grid-cols-2 gap-3">
            <Field label="来源类型">
              <select
                value={sourceType}
                onChange={(e) => setSourceType(e.target.value)}
                className="h-10 w-full rounded-[10px] border border-line-strong bg-paper-raised px-3 text-[12px]"
              >
                <option value="author">作者原创</option>
                <option value="licensed">已授权</option>
                <option value="public_domain">公版</option>
                <option value="import">导入稿件</option>
                <option value="chat">对话采纳</option>
              </select>
            </Field>
            <Field label="来源标记">
              <Input
                value={sourceSpan}
                onChange={(e) => setSourceSpan(e.target.value)}
                placeholder="例如：第 3 章 / 作者样本 2"
              />
            </Field>
          </div>
          <Field label="台词或 POV 样本" hint="至少 20 字">
            <Textarea
              rows={8}
              value={sampleText}
              onChange={(e) => setSampleText(e.target.value)}
            />
          </Field>
        </div>
      </Modal>

      <Modal
        open={checkOpen}
        onOpenChange={setCheckOpen}
        title="检查人物声音漂移"
        description="系统只抽取署名给该人物的对话；若提供场景且该人物是 POV，也会抽取 POV 段落。"
        width="max-w-[820px]"
        footer={
          <>
            <Button tone="ghost" onClick={() => setCheckOpen(false)}>
              关闭
            </Button>
            <Button tone="primary" onClick={() => void runCheck()} disabled={busy || !checkText.trim()}>
              {busy ? <LoaderCircle size={12} className="animate-spin" /> : null}
              运行检查
            </Button>
          </>
        }
      >
        <Field label="待检查文本">
          <Textarea rows={9} value={checkText} onChange={(e) => setCheckText(e.target.value)} />
        </Field>
        {check ? (
          <div className="mt-3 space-y-2">
            {check.segments.map((segment) => (
              <div key={`${segment.index}-${segment.start}`} className="rounded-lg border border-line bg-paper p-2.5">
                <div className="flex items-center justify-between text-[9px] text-muted">
                  <span>{segment.kind === "dialogue" ? "人物台词" : "POV 段落"} · 片段 {segment.start}–{segment.end}</span>
                  <span>句长 {segment.metrics.avgSentenceLen}</span>
                </div>
                <div className="mt-1 text-[10.5px] leading-5 text-ink-soft">{segment.excerpt}</div>
              </div>
            ))}
            {!check.drifts.length ? (
              <div className="rounded-lg bg-teal-soft/50 px-3 py-2 text-[10px] text-teal">
                当前片段未超过该人物声音阈值。
              </div>
            ) : null}
          </div>
        ) : null}
      </Modal>
    </div>
  );
}
