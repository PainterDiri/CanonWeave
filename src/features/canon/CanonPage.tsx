import { useCallback, useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import {
  BookMarked,
  FileText,
  FolderOpen,
  LoaderCircle,
  Paintbrush,
  Plug,
  RefreshCw,
  ShieldAlert,
  Sparkles,
} from "lucide-react";
import { Badge, Button, EmptyState, Field, Input, SectionTitle } from "@/components/ui";
import { ipc, isTauri, type ModelConfig, type SourceDocument } from "@/lib/ipc";
import { useAppStore } from "@/stores/ui";

type Tab = "model" | "import" | "style" | "rules";

export function CanonPage({ model }: { model: ModelConfig | null }) {
  const [tab, setTab] = useState<Tab>("model");
  return (
    <div className="fade-in mx-auto max-w-[1060px] px-8 py-7">
      <SectionTitle eyebrow="Canon · 设定" title="设定与项目配置" />
      <div className="mb-5 flex items-center gap-1 border-b border-line">
        {[
          { key: "model" as const, label: "模型", icon: Plug },
          { key: "import" as const, label: "旧稿导入", icon: FileText },
          { key: "style" as const, label: "文笔", icon: Paintbrush },
          { key: "rules" as const, label: "规则", icon: ShieldAlert },
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
      {tab === "model" ? <ModelPanel model={model} /> : null}
      {tab === "import" ? <ImportPanel /> : null}
      {tab === "style" ? (
        <EmptyState
          icon={<Paintbrush size={20} />}
          title="文笔档案将在下一阶段接入"
          description="文笔档案记录视角距离、句法节奏、词汇、意象、修辞、情绪表达与禁例，并能导出为结构化文笔契约。"
          action={<Badge tone="amber">M2</Badge>}
        />
      ) : null}
      {tab === "rules" ? (
        <EmptyState
          icon={<ShieldAlert size={20} />}
          title="规则编译器将在下一阶段接入"
          description="硬规则会编译为可执行谓词；复杂时间与资源约束交给 Z3，并返回可映射回原文的冲突核心。"
          action={<Badge tone="amber">M3</Badge>}
        />
      ) : null}
    </div>
  );
}

function ModelPanel({ model }: { model: ModelConfig | null }) {
  const setModel = useAppStore((s) => s.setModel);
  const [baseUrl, setBaseUrl] = useState(model?.baseUrl ?? "https://api.openai.com/v1");
  const [modelName, setModelName] = useState(model?.model ?? "gpt-4o-mini");
  const [apiKey, setApiKey] = useState("");
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<{ ok: boolean; text: string } | null>(null);

  const save = async () => {
    setBusy(true);
    try {
      const saved = await ipc.modelSave({
        baseUrl,
        model: modelName,
        temperature: model?.temperature ?? 0.8,
        maxTokens: model?.maxTokens ?? 4096,
        embeddingModel: model?.embeddingModel ?? null,
        apiKey: apiKey.trim() || null,
      });
      setModel(saved);
      setApiKey("");
      setMessage({ ok: true, text: "配置已保存" });
    } catch (e) {
      setMessage({ ok: false, text: e instanceof Error ? e.message : String(e) });
    } finally {
      setBusy(false);
    }
  };

  const test = async () => {
    setBusy(true);
    try {
      await save();
      const result = await ipc.modelTest();
      setMessage({ ok: result.ok, text: `${result.message}（${result.latencyMs} ms）` });
    } catch (e) {
      setMessage({ ok: false, text: e instanceof Error ? e.message : String(e) });
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="paper-card max-w-[680px] p-6">
      <div className="mb-5 flex items-center justify-between">
        <div>
          <div className="text-[14px] font-semibold text-ink">OpenAI 兼容接口</div>
          <div className="mt-1 text-[11px] text-muted">
            默认使用 /chat/completions；密钥保存在系统钥匙串。
          </div>
        </div>
        <Badge tone={model?.hasApiKey ? "teal" : "amber"}>
          {model?.hasApiKey ? "密钥已保存" : "尚未保存密钥"}
        </Badge>
      </div>
      <div className="grid grid-cols-2 gap-3">
        <Field label="Base URL">
          <Input value={baseUrl} onChange={(e) => setBaseUrl(e.target.value)} />
        </Field>
        <Field label="模型名">
          <Input value={modelName} onChange={(e) => setModelName(e.target.value)} />
        </Field>
      </div>
      <div className="mt-3">
        <Field label="API Key" hint="留空保持当前密钥">
          <Input
            type="password"
            value={apiKey}
            onChange={(e) => setApiKey(e.target.value)}
            placeholder={model?.hasApiKey ? "已保存" : "sk-..."}
            autoComplete="off"
          />
        </Field>
      </div>
      <div className="mt-4 flex items-center justify-between">
        <div className="text-[11px] leading-5">
          {message ? (
            <span className={message.ok ? "text-teal" : "text-coral"}>{message.text}</span>
          ) : (
            <span className="text-muted">测试会发送一次极短请求</span>
          )}
        </div>
        <div className="flex gap-2">
          <Button size="sm" onClick={() => void save()} disabled={busy}>
            保存
          </Button>
          <Button size="sm" tone="primary" onClick={() => void test()} disabled={busy}>
            {busy ? <LoaderCircle size={12} className="animate-spin" /> : null}
            测试连接
          </Button>
        </div>
      </div>
      <div className="mt-5 rounded-lg border border-line bg-paper px-3.5 py-3 text-[10.5px] leading-5 text-muted">
        未配置嵌入模型时，全文检索与图谱检索仍然可用；向量召回会自动降级。
      </div>
    </section>
  );
}

function ImportPanel() {
  const [documents, setDocuments] = useState<SourceDocument[]>([]);
  const [busy, setBusy] = useState(false);
  const [extracting, setExtracting] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const refreshProject = useAppStore((s) => s.refreshProject);

  const load = useCallback(async () => {
    try {
      setDocuments(await ipc.documentList());
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const choose = async () => {
    if (!isTauri()) {
      setError("旧稿导入需要在桌面应用中进行");
      return;
    }
    const selected = await open({
      multiple: false,
      filters: [
        { name: "稿件", extensions: ["txt", "md", "markdown", "docx"] },
      ],
      title: "选择要导入的稿件",
    });
    if (typeof selected !== "string") return;
    setBusy(true);
    setError(null);
    setMessage(null);
    try {
      const result = await ipc.documentImport(selected);
      setMessage(
        `已导入《${result.document.title}》：${result.document.wordCount} 字，${result.chaptersCreated} 章。接下来运行事实抽取。`,
      );
      await load();
      await refreshProject();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const extract = async (document: SourceDocument) => {
    setExtracting(document.id);
    setError(null);
    setMessage(null);
    try {
      const result = await ipc.extractionRun(document.id);
      setMessage(`抽取完成：生成 ${result.candidatesCreated} 条待确认候选，请到审计页逐条确认。`);
      await refreshProject();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setExtracting(null);
    }
  };

  return (
    <div className="space-y-4">
      <section className="paper-card p-6">
        <div className="flex items-start justify-between">
          <div>
            <div className="flex items-center gap-2 text-[14px] font-semibold text-ink">
              <BookMarked size={15} className="text-teal" />
              导入旧稿
            </div>
            <p className="mt-1.5 max-w-xl text-[11.5px] leading-6 text-muted">
              支持 TXT、Markdown 与 DOCX。原文件会复制到项目的 imports/ 目录；正文按“第 X 章”切分并建立全文索引。
            </p>
          </div>
          <Button tone="primary" onClick={() => void choose()} disabled={busy} icon={<FolderOpen size={13} />}>
            {busy ? "正在导入…" : "选择文件"}
          </Button>
        </div>
        {message ? (
          <div className="mt-4 rounded-lg border border-teal/20 bg-teal-soft/50 px-3.5 py-2.5 text-[11px] leading-5 text-teal">
            {message}
          </div>
        ) : null}
        {error ? (
          <div className="mt-4 rounded-lg border border-coral/20 bg-coral-soft/50 px-3.5 py-2.5 text-[11px] leading-5 text-coral">
            {error}
          </div>
        ) : null}
      </section>

      {documents.length === 0 ? (
        <EmptyState
          icon={<FileText size={20} />}
          title="还没有导入稿件"
          description="导入后先生成事实候选，只有你确认过的内容才会进入正式事实表。"
        />
      ) : (
        <div className="space-y-2">
          {documents.map((document) => (
            <article key={document.id} className="paper-card flex items-center justify-between px-5 py-4">
              <div className="flex items-start gap-3">
                <div className="flex h-9 w-9 items-center justify-center rounded-lg bg-paper text-teal">
                  <FileText size={15} />
                </div>
                <div>
                  <div className="text-[12.5px] font-medium text-ink">{document.title}</div>
                  <div className="mt-1 flex items-center gap-2 text-[10px] text-muted">
                    <span>{document.docType.toUpperCase()}</span>
                    <span>·</span>
                    <span>{document.wordCount.toLocaleString()} 字</span>
                    <span>·</span>
                    <span>{new Date(document.importedAt).toLocaleString("zh-CN")}</span>
                  </div>
                </div>
              </div>
              <Button
                size="sm"
                onClick={() => void extract(document)}
                disabled={extracting === document.id}
                icon={
                  extracting === document.id ? (
                    <LoaderCircle size={12} className="animate-spin" />
                  ) : (
                    <Sparkles size={12} />
                  )
                }
              >
                {extracting === document.id ? "正在抽取…" : "抽取事实候选"}
              </Button>
            </article>
          ))}
        </div>
      )}
      <div className="flex items-center gap-2 text-[10px] text-muted">
        <RefreshCw size={11} />
        抽取只会写入候选区，不会直接成为正式事实。
      </div>
    </div>
  );
}
