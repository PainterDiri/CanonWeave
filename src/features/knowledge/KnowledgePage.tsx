import { useCallback, useEffect, useMemo, useState } from "react";
import {
  BookOpen,
  Boxes,
  Landmark,
  LoaderCircle,
  Package,
  Pencil,
  Plus,
  Sparkles,
} from "lucide-react";
import {
  Badge,
  Button,
  EmptyState,
  Field,
  Input,
  Modal,
  SectionTitle,
  Textarea,
} from "@/components/ui";
import { AgentSuggestion } from "@/components/AgentSuggestion";
import {
  ipc,
  type KnowledgeDomain,
  type KnowledgeDomainInput,
  type KnowledgeEntry,
} from "@/lib/ipc";
import { useAppStore } from "@/stores/ui";

type EntryDraft = {
  id: string | null;
  domainId: string;
  name: string;
  aliases: string;
  summary: string;
  fields: Record<string, string>;
};

const domainIcons: Record<string, typeof BookOpen> = {
  landmark: Landmark,
  package: Package,
  sparkles: Sparkles,
  book: BookOpen,
};

export function KnowledgePage() {
  const [domains, setDomains] = useState<KnowledgeDomain[]>([]);
  const [entries, setEntries] = useState<KnowledgeEntry[]>([]);
  const [selectedDomain, setSelectedDomain] = useState<string | null>(null);
  const [selectedEntry, setSelectedEntry] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [domainDraft, setDomainDraft] = useState<KnowledgeDomainInput | null>(null);
  const [entryDraft, setEntryDraft] = useState<EntryDraft | null>(null);
  const [busy, setBusy] = useState(false);
  const notify = useAppStore((state) => state.notify);
  const refreshProject = useAppStore((state) => state.refreshProject);

  const loadDomains = useCallback(async (preferred?: string | null) => {
    const list = await ipc.knowledgeDomains();
    setDomains(list);
    setSelectedDomain((current) => {
      const next = preferred ?? current;
      return list.some((domain) => domain.id === next) ? next : (list[0]?.id ?? null);
    });
  }, []);

  const loadEntries = useCallback(async (domainId: string | null) => {
    const list = await ipc.knowledgeEntries(domainId);
    setEntries(list);
    setSelectedEntry((current) =>
      list.some((entry) => entry.id === current) ? current : (list[0]?.id ?? null),
    );
  }, []);

  useEffect(() => {
    void (async () => {
      try {
        await loadDomains();
      } catch (cause) {
        setError(cause instanceof Error ? cause.message : String(cause));
      } finally {
        setLoading(false);
      }
    })();
  }, [loadDomains]);

  useEffect(() => {
    if (!selectedDomain) {
      setEntries([]);
      return;
    }
    void loadEntries(selectedDomain).catch((cause) =>
      setError(cause instanceof Error ? cause.message : String(cause)),
    );
  }, [loadEntries, selectedDomain]);

  const activeDomain = domains.find((domain) => domain.id === selectedDomain) ?? domains[0] ?? null;
  const activeEntry = entries.find((entry) => entry.id === selectedEntry) ?? entries[0] ?? null;

  const openNewEntry = () => {
    if (!activeDomain) return;
    setEntryDraft({
      id: null,
      domainId: activeDomain.id,
      name: "",
      aliases: "",
      summary: "",
      fields: Object.fromEntries(activeDomain.fieldSchema.map((field) => [field.key, ""])),
    });
  };

  const openEditEntry = (entry: KnowledgeEntry) => {
    const domain = domains.find((item) => item.id === entry.domainId);
    if (!domain) return;
    setEntryDraft({
      id: entry.id,
      domainId: entry.domainId,
      name: entry.name,
      aliases: entry.aliases.join("，"),
      summary: entry.summary,
      fields: Object.fromEntries(
        domain.fieldSchema.map((field) => [
          field.key,
          String(entry.fields?.[field.key] ?? ""),
        ]),
      ),
    });
  };

  const saveDomain = async () => {
    if (!domainDraft?.name.trim()) return;
    setBusy(true);
    try {
      const created = await ipc.knowledgeDomainCreate(domainDraft);
      setDomainDraft(null);
      await loadDomains(created.id);
      await loadEntries(created.id);
      await refreshProject();
      notify(`已建立知识域：${created.name}`);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setBusy(false);
    }
  };

  const saveEntry = async () => {
    if (!entryDraft?.name.trim()) return;
    setBusy(true);
    try {
      const saved = await ipc.knowledgeEntrySave({
        id: entryDraft.id,
        domainId: entryDraft.domainId,
        name: entryDraft.name,
        aliases: entryDraft.aliases
          .split(/[，,\n]/)
          .map((value) => value.trim())
          .filter(Boolean),
        summary: entryDraft.summary,
        fields: entryDraft.fields,
      });
      setEntryDraft(null);
      await loadDomains(entryDraft.domainId);
      await loadEntries(entryDraft.domainId);
      setSelectedEntry(saved.id);
      await refreshProject();
      notify("知识条目已写入本地知识库");
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setBusy(false);
    }
  };

  const currentFields = useMemo(
    () =>
      activeDomain?.fieldSchema.map((field) => ({
        ...field,
        value: activeEntry?.fields?.[field.key],
      })) ?? [],
    [activeDomain, activeEntry],
  );

  return (
    <div className="fade-in mx-auto max-w-[1240px] px-7 py-6">
      <SectionTitle
        title="知识库"
        action={
          <div className="flex items-center gap-2">
            <Button
              size="sm"
              onClick={() =>
                setDomainDraft({ name: "", description: "", fields: [] })
              }
              icon={<Plus size={13} />}
            >
              新建知识域
            </Button>
            <Button
              size="sm"
              tone="primary"
              onClick={openNewEntry}
              disabled={!activeDomain}
              icon={<Plus size={13} />}
            >
              新建条目
            </Button>
          </div>
        }
      />
      <AgentSuggestion outputs={["all", "knowledge", "outline"]} />
      <p className="-mt-2 mb-5 max-w-3xl text-[12px] leading-6 text-muted">
        环境、重要物品和自建知识域与人物并列。条目会自动成为图谱实体，并进入全文检索、上下文编译和
        Narrative Wiki；作者确认后的结构化字段才是正式设定。
      </p>
      {error ? (
        <div className="mb-4 rounded-lg border border-coral/25 bg-coral-soft/60 px-4 py-2.5 text-[11.5px] text-coral">
          {error}
        </div>
      ) : null}
      {loading ? (
        <div className="flex h-64 items-center justify-center text-[12px] text-muted">
          <LoaderCircle size={15} className="mr-2 animate-spin" /> 正在读取知识域
        </div>
      ) : (
        <div className="grid grid-cols-[248px_minmax(0,1fr)] gap-4">
          <aside className="paper-card h-[650px] overflow-y-auto p-2.5">
            <div className="px-2 pb-2 pt-1">
              <div className="text-[10px] font-semibold tracking-[0.16em] text-muted">
                故事资料类型
              </div>
            </div>
            {domains.map((domain) => {
              const Icon = domainIcons[domain.icon] ?? Boxes;
              const active = activeDomain?.id === domain.id;
              return (
                <button
                  key={domain.id}
                  onClick={() => setSelectedDomain(domain.id)}
                  className={`focus-ring mb-1 flex w-full items-center gap-3 rounded-lg px-3 py-2.5 text-left transition ${
                    active ? "bg-teal text-paper-raised" : "hover:bg-ink/5"
                  }`}
                >
                  <Icon size={15} className={active ? "text-paper-raised" : "text-teal/70"} />
                  <span className="min-w-0 flex-1">
                    <span className="block truncate text-[12px] font-medium">{domain.name}</span>
                    <span
                      className={`mt-0.5 block text-[9.5px] ${
                        active ? "text-paper-raised/65" : "text-muted"
                      }`}
                    >
                      {domain.isSystem ? "内置知识域" : "作者定义"}
                    </span>
                  </span>
                  <span
                    className={`text-[10px] tabular-nums ${
                      active ? "text-paper-raised/75" : "text-muted"
                    }`}
                  >
                    {domain.entryCount}
                  </span>
                </button>
              );
            })}
          </aside>

          <section className="paper-card h-[650px] overflow-hidden">
            {activeDomain ? (
              <div className="grid h-full grid-cols-[300px_minmax(0,1fr)]">
                <div className="flex min-h-0 flex-col border-r border-line">
                  <div className="border-b border-line px-4 py-4">
                    <div className="flex items-center justify-between gap-2">
                      <h2 className="display text-[20px] text-ink">{activeDomain.name}</h2>
                      <Badge tone={activeDomain.isSystem ? "neutral" : "amber"}>
                        {activeDomain.isSystem ? "内置" : "自定义"}
                      </Badge>
                    </div>
                    <p className="mt-1.5 text-[10.5px] leading-5 text-muted">
                      {activeDomain.description}
                    </p>
                  </div>
                  <div className="min-h-0 flex-1 overflow-y-auto p-2">
                    {entries.length ? (
                      entries.map((entry) => (
                        <button
                          key={entry.id}
                          onClick={() => setSelectedEntry(entry.id)}
                          className={`focus-ring mb-1 w-full rounded-lg px-3 py-2.5 text-left transition ${
                            activeEntry?.id === entry.id
                              ? "bg-teal-soft/80"
                              : "hover:bg-ink/[0.035]"
                          }`}
                        >
                          <div className="truncate text-[12px] font-medium text-ink">
                            {entry.name}
                          </div>
                          <div className="mt-1 line-clamp-2 text-[10px] leading-4 text-muted">
                            {entry.summary || "尚无摘要"}
                          </div>
                        </button>
                      ))
                    ) : (
                      <div className="px-3 py-10 text-center text-[10.5px] leading-5 text-muted">
                        这个知识域还没有条目。先记录一个地点、物品、能力或规则。
                      </div>
                    )}
                  </div>
                </div>

                <div className="min-h-0 overflow-y-auto px-6 py-5">
                  {activeEntry ? (
                    <>
                      <div className="flex items-start justify-between gap-4 border-b border-line pb-4">
                        <div>
                          <div className="flex items-center gap-2">
                            <h3 className="display text-[24px] text-ink">{activeEntry.name}</h3>
                            <Badge tone="teal">{activeEntry.domainName}</Badge>
                          </div>
                          <div className="mt-1 text-[10.5px] text-muted">
                            {activeEntry.aliases.length
                              ? `别名：${activeEntry.aliases.join("、")}`
                              : "暂无别名"}
                          </div>
                        </div>
                        <Button
                          size="sm"
                          onClick={() => openEditEntry(activeEntry)}
                          icon={<Pencil size={12} />}
                        >
                          编辑条目
                        </Button>
                      </div>
                      <p className="mt-4 text-[12px] leading-6 text-ink-soft">
                        {activeEntry.summary || "尚未填写摘要。"}
                      </p>
                      <div className="mt-5 grid grid-cols-2 gap-x-5 gap-y-4">
                        {currentFields.map((field) => (
                          <div key={field.key} className="border-t border-line pt-3">
                            <div className="text-[9.5px] tracking-wide text-muted">
                              {field.label}
                            </div>
                            <div className="mt-1.5 whitespace-pre-wrap text-[11.5px] leading-6 text-ink-soft">
                              {field.value || <span className="text-muted/60">未填写</span>}
                            </div>
                          </div>
                        ))}
                      </div>
                      <div className="mt-6 flex items-center gap-2 text-[9.5px] text-muted">
                        <span className="h-1.5 w-1.5 rounded-full bg-teal" />
                        已作为图谱实体进入检索；Wiki 重建时会生成派生页面。
                      </div>
                    </>
                  ) : (
                    <EmptyState
                      icon={<BookOpen size={20} />}
                      title={`建立第一个${activeDomain.name}条目`}
                      description="使用可视字段记录知识，不把关键设定埋在一段长文本里。"
                      action={
                        <Button tone="primary" onClick={openNewEntry} icon={<Plus size={13} />}>
                          新建条目
                        </Button>
                      }
                    />
                  )}
                </div>
              </div>
            ) : (
              <EmptyState
                icon={<Boxes size={20} />}
                title="还没有知识域"
                description="建立环境、重要物品或契合作品世界观的专用知识类型。"
              />
            )}
          </section>
        </div>
      )}

      <DomainEditor
        draft={domainDraft}
        onChange={setDomainDraft}
        onClose={() => setDomainDraft(null)}
        onSave={saveDomain}
        busy={busy}
      />
      <EntryEditor
        draft={entryDraft}
        domain={activeDomain}
        onChange={setEntryDraft}
        onClose={() => setEntryDraft(null)}
        onSave={saveEntry}
        busy={busy}
      />
    </div>
  );
}

function DomainEditor({
  draft,
  onChange,
  onClose,
  onSave,
  busy,
}: {
  draft: KnowledgeDomainInput | null;
  onChange: (draft: KnowledgeDomainInput | null) => void;
  onClose: () => void;
  onSave: () => Promise<void>;
  busy: boolean;
}) {
  return (
    <Modal
      open={Boolean(draft)}
      onOpenChange={(open) => !open && onClose()}
      title="新建知识域"
      description="适合禁墟、神道、灾厄、组织体系等作品专属知识。字段将决定条目的录入结构。"
      footer={
        <>
          <Button tone="ghost" onClick={onClose}>
            取消
          </Button>
          <Button tone="primary" onClick={() => void onSave()} disabled={busy || !draft?.name.trim()}>
            建立知识域
          </Button>
        </>
      }
    >
      {draft ? (
        <div className="space-y-3">
          <Field label="知识域名称">
            <Input
              value={draft.name}
              onChange={(event) => onChange({ ...draft, name: event.target.value })}
              placeholder="例如：禁墟"
              autoFocus
            />
          </Field>
          <Field label="用途说明">
            <Textarea
              rows={2}
              value={draft.description}
              onChange={(event) => onChange({ ...draft, description: event.target.value })}
              placeholder="这个知识域记录什么，如何影响剧情。"
            />
          </Field>
          <Field label="字段" hint="每行一个，留空则使用核心设定 / 规则限制 / 关联影响">
            <Textarea
              rows={5}
              value={draft.fields.join("\n")}
              onChange={(event) =>
                onChange({
                  ...draft,
                  fields: event.target.value.split("\n"),
                })
              }
              placeholder={"能力\n代价\n克制关系\n现状"}
            />
          </Field>
        </div>
      ) : null}
    </Modal>
  );
}

function EntryEditor({
  draft,
  domain,
  onChange,
  onClose,
  onSave,
  busy,
}: {
  draft: EntryDraft | null;
  domain: KnowledgeDomain | null;
  onChange: (draft: EntryDraft | null) => void;
  onClose: () => void;
  onSave: () => Promise<void>;
  busy: boolean;
}) {
  return (
    <Modal
      open={Boolean(draft)}
      onOpenChange={(open) => !open && onClose()}
      title={draft?.id ? "编辑知识条目" : `新建${domain?.name ?? "知识"}条目`}
      description="字段保存后会成为图谱实体并参与检索。设定变化仍需经过作者明确编辑。"
      width="max-w-[720px]"
      footer={
        <>
          <Button tone="ghost" onClick={onClose}>
            取消
          </Button>
          <Button tone="primary" onClick={() => void onSave()} disabled={busy || !draft?.name.trim()}>
            保存条目
          </Button>
        </>
      }
    >
      {draft && domain ? (
        <div className="space-y-3">
          <div className="grid grid-cols-2 gap-3">
            <Field label="名称">
              <Input
                value={draft.name}
                onChange={(event) => onChange({ ...draft, name: event.target.value })}
                autoFocus
              />
            </Field>
            <Field label="别名" hint="逗号分隔">
              <Input
                value={draft.aliases}
                onChange={(event) => onChange({ ...draft, aliases: event.target.value })}
              />
            </Field>
          </div>
          <Field label="摘要">
            <Textarea
              rows={2}
              value={draft.summary}
              onChange={(event) => onChange({ ...draft, summary: event.target.value })}
            />
          </Field>
          <div className="grid grid-cols-2 gap-3">
            {domain.fieldSchema.map((field) => (
              <Field key={field.key} label={field.label}>
                <Textarea
                  rows={3}
                  value={draft.fields[field.key] ?? ""}
                  onChange={(event) =>
                    onChange({
                      ...draft,
                      fields: { ...draft.fields, [field.key]: event.target.value },
                    })
                  }
                  placeholder={field.placeholder}
                />
              </Field>
            ))}
          </div>
        </div>
      ) : null}
    </Modal>
  );
}
