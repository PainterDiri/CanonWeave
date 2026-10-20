import { useCallback, useEffect, useState } from "react";
import { LoaderCircle, Plus, Sparkles, UserRound } from "lucide-react";
import { Badge, Button, EmptyState, Field, Input, Modal, SectionTitle, Textarea } from "@/components/ui";
import { ipc, type CharacterDetail } from "@/lib/ipc";
import { useAppStore } from "@/stores/ui";

type Draft = {
  entityId: string | null;
  canonicalName: string;
  aliases: string;
  description: string;
  desire: string;
  need: string;
  fear: string;
  flaw: string;
  values: string;
  secret: string;
  taboo: string;
  voiceAnchor: string;
};

const blank: Draft = {
  entityId: null,
  canonicalName: "",
  aliases: "",
  description: "",
  desire: "",
  need: "",
  fear: "",
  flaw: "",
  values: "",
  secret: "",
  taboo: "",
  voiceAnchor: "",
};

export function CharactersPage() {
  const [characters, setCharacters] = useState<CharacterDetail[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [editing, setEditing] = useState<Draft | null>(null);
  const [error, setError] = useState<string | null>(null);
  const refreshProject = useAppStore((s) => s.refreshProject);

  const load = useCallback(async () => {
    try {
      const list = await ipc.characterList();
      setCharacters(list);
      setSelected((current) =>
        current && list.some((c) => c.entity.id === current)
          ? current
          : (list[0]?.entity.id ?? null),
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

  const current = characters.find((c) => c.entity.id === selected) ?? characters[0] ?? null;

  const openNew = () => setEditing({ ...blank });
  const openEdit = (character: CharacterDetail) => {
    setEditing({
      entityId: character.entity.id,
      canonicalName: character.entity.canonicalName,
      aliases: character.entity.aliases.join("，"),
      description: character.entity.description,
      desire: character.desire,
      need: character.need,
      fear: character.fear,
      flaw: character.flaw,
      values: character.values.join("，"),
      secret: character.secret,
      taboo: character.taboo.join("，"),
      voiceAnchor: character.voiceAnchor,
    });
  };

  const save = async () => {
    if (!editing) return;
    try {
      await ipc.characterSave({
        entityId: editing.entityId,
        canonicalName: editing.canonicalName,
        aliases: editing.aliases.split(/[，,\n]/).map((s) => s.trim()).filter(Boolean),
        description: editing.description,
        desire: editing.desire,
        need: editing.need,
        fear: editing.fear,
        flaw: editing.flaw,
        values: editing.values.split(/[，,\n]/).map((s) => s.trim()).filter(Boolean),
        secret: editing.secret,
        taboo: editing.taboo.split(/[，,\n]/).map((s) => s.trim()).filter(Boolean),
        voiceAnchor: editing.voiceAnchor,
      });
      setEditing(null);
      await load();
      await refreshProject();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  return (
    <div className="fade-in mx-auto max-w-[1180px] px-8 py-7">
      <SectionTitle
        eyebrow="Characters"
        title="人物"
        action={
          <Button tone="primary" size="sm" onClick={openNew} icon={<Plus size={13} />}>
            新建人物
          </Button>
        }
      />
      {error ? (
        <div className="mb-4 rounded-lg border border-coral/25 bg-coral-soft/60 px-4 py-2.5 text-[12px] text-coral">
          {error}
        </div>
      ) : null}
      {loading ? (
        <div className="flex h-64 items-center justify-center text-[12px] text-muted">
          <LoaderCircle size={15} className="mr-2 animate-spin" /> 正在读取人物卡
        </div>
      ) : characters.length === 0 ? (
        <EmptyState
          icon={<UserRound size={20} />}
          title="人物尚未出场"
          description="人物卡记录欲望、需求、恐惧、缺陷、秘密与底线。作者认可的声音样本会用于之后的声音漂移检查。"
          action={
            <Button tone="primary" onClick={openNew} icon={<Plus size={13} />}>
              建立第一个人物
            </Button>
          }
        />
      ) : (
        <div className="grid grid-cols-[300px_minmax(0,1fr)] gap-4">
          <section className="paper-card flex h-[560px] min-h-0 flex-col overflow-hidden">
            <div className="border-b border-line px-4 py-2.5 text-[11px] font-semibold text-ink-soft">
              人物 {characters.length}
            </div>
            <div className="min-h-0 flex-1 overflow-y-auto p-1.5">
              {characters.map((character) => (
                <button
                  key={character.entity.id}
                  onClick={() => setSelected(character.entity.id)}
                  className={`mb-1 flex w-full items-center gap-3 rounded-lg px-2.5 py-2.5 text-left transition ${
                    current?.entity.id === character.entity.id
                      ? "bg-teal-soft/70"
                      : "hover:bg-ink/[0.03]"
                  }`}
                >
                  <div
                    className={`flex h-9 w-9 shrink-0 items-center justify-center rounded-full font-display text-[14px] ${
                      current?.entity.id === character.entity.id
                        ? "bg-teal text-paper-raised"
                        : "bg-ink/8 text-ink-soft"
                    }`}
                  >
                    {character.entity.canonicalName.slice(0, 1)}
                  </div>
                  <div className="min-w-0">
                    <div className="truncate text-[12px] font-medium text-ink">
                      {character.entity.canonicalName}
                    </div>
                    <div className="mt-0.5 truncate text-[10px] text-muted">
                      {character.desire || "欲望未设定"}
                    </div>
                  </div>
                </button>
              ))}
            </div>
          </section>

          {current ? (
            <section className="paper-card flex h-[560px] min-h-0 flex-col overflow-hidden">
              <div className="flex items-start justify-between border-b border-line px-6 py-5">
                <div>
                  <div className="flex items-center gap-2">
                    <h2 className="display text-[24px] text-ink">{current.entity.canonicalName}</h2>
                    <Badge tone="teal">人物</Badge>
                  </div>
                  <div className="mt-1 text-[11px] text-muted">
                    {current.entity.aliases.length
                      ? `别名：${current.entity.aliases.join("、")}`
                      : "暂无别名"}
                  </div>
                </div>
                <Button size="sm" onClick={() => openEdit(current)}>
                  编辑人物卡
                </Button>
              </div>
              <div className="min-h-0 flex-1 overflow-y-auto px-6 py-5">
                <p className="mb-5 text-[12px] leading-6 text-ink-soft">
                  {current.entity.description || "还没有人物描述。"}
                </p>
                <div className="grid grid-cols-3 gap-3">
                  {[
                    ["欲望", current.desire],
                    ["需求", current.need],
                    ["恐惧", current.fear],
                    ["缺陷", current.flaw],
                    ["秘密", current.secret],
                    ["底线", current.taboo.join("、")],
                  ].map(([label, value]) => (
                    <div key={label} className="rounded-xl border border-line bg-paper/60 p-3.5">
                      <div className="text-[9.5px] tracking-wide text-muted">{label}</div>
                      <div className="mt-1.5 text-[11.5px] leading-5 text-ink-soft">
                        {value || <span className="text-muted/60">未设定</span>}
                      </div>
                    </div>
                  ))}
                </div>
                <div className="mt-5 rounded-xl border border-line bg-paper/60 p-4">
                  <div className="mb-2 flex items-center gap-1.5 text-[10px] font-medium tracking-wide text-muted">
                    <Sparkles size={10} className="text-amber" />
                    声音锚点 · 作者认可的样本
                  </div>
                  <p className="text-[11.5px] leading-6 whitespace-pre-wrap text-ink-soft">
                    {current.voiceAnchor || "还没有样本。之后贴入 3 至 8 段作者认可的对话，用于声音漂移检查。"}
                  </p>
                </div>
                {current.values.length ? (
                  <div className="mt-4 flex flex-wrap gap-1.5">
                    {current.values.map((value) => (
                      <span key={value} className="rounded-full bg-teal-soft px-2.5 py-0.5 text-[10px] text-teal">
                        {value}
                      </span>
                    ))}
                  </div>
                ) : null}
              </div>
            </section>
          ) : null}
        </div>
      )}

      <CharacterEditor
        draft={editing}
        onChange={setEditing}
        onClose={() => setEditing(null)}
        onSave={save}
      />
    </div>
  );
}

function CharacterEditor({
  draft,
  onChange,
  onClose,
  onSave,
}: {
  draft: Draft | null;
  onChange: (draft: Draft) => void;
  onClose: () => void;
  onSave: () => Promise<void>;
}) {
  const set = <K extends keyof Draft>(key: K, value: Draft[K]) => {
    if (draft) onChange({ ...draft, [key]: value });
  };
  return (
    <Modal
      open={Boolean(draft)}
      onOpenChange={(open) => !open && onClose()}
      title={draft?.entityId ? "编辑人物卡" : "新建人物"}
      description="人物核心用于检查动机与底线；声音样本只回答“这句话像谁说的”。"
      width="max-w-[700px]"
      footer={
        <>
          <Button tone="ghost" onClick={onClose}>
            取消
          </Button>
          <Button tone="primary" onClick={() => void onSave()} disabled={!draft?.canonicalName.trim()}>
            保存人物
          </Button>
        </>
      }
    >
      {draft ? (
        <div className="space-y-3">
          <div className="grid grid-cols-2 gap-3">
            <Field label="姓名">
              <Input
                value={draft.canonicalName}
                onChange={(e) => set("canonicalName", e.target.value)}
              />
            </Field>
            <Field label="别名" hint="逗号分隔">
              <Input value={draft.aliases} onChange={(e) => set("aliases", e.target.value)} />
            </Field>
          </div>
          <Field label="人物描述">
            <Textarea
              rows={2}
              value={draft.description}
              onChange={(e) => set("description", e.target.value)}
            />
          </Field>
          <div className="grid grid-cols-2 gap-3">
            <Field label="欲望">
              <Input value={draft.desire} onChange={(e) => set("desire", e.target.value)} />
            </Field>
            <Field label="需求">
              <Input value={draft.need} onChange={(e) => set("need", e.target.value)} />
            </Field>
            <Field label="恐惧">
              <Input value={draft.fear} onChange={(e) => set("fear", e.target.value)} />
            </Field>
            <Field label="缺陷">
              <Input value={draft.flaw} onChange={(e) => set("flaw", e.target.value)} />
            </Field>
            <Field label="秘密">
              <Input value={draft.secret} onChange={(e) => set("secret", e.target.value)} />
            </Field>
            <Field label="价值观" hint="逗号分隔">
              <Input value={draft.values} onChange={(e) => set("values", e.target.value)} />
            </Field>
          </div>
          <Field label="底线 / 禁忌" hint="逗号分隔">
            <Input value={draft.taboo} onChange={(e) => set("taboo", e.target.value)} />
          </Field>
          <Field label="声音锚点" hint="作者认可的语气样本">
            <Textarea
              rows={4}
              value={draft.voiceAnchor}
              onChange={(e) => set("voiceAnchor", e.target.value)}
              placeholder="贴入 1 至 3 段这个人物的对话或视角段落…"
            />
          </Field>
        </div>
      ) : null}
    </Modal>
  );
}
