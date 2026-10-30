import { useCallback, useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import {
  BookMarked,
  Download,
  FileText,
  FolderOpen,
  LoaderCircle,
  Paintbrush,
  Plug,
  RefreshCw,
  ScrollText,
  ShieldAlert,
  Sparkles,
} from "lucide-react";
import { Badge, Button, EmptyState, Field, Input, SectionTitle } from "@/components/ui";
import {
  ipc,
  isTauri,
  onEmbeddingProgress,
  type EmbeddingStatus,
  type ExportResult,
  type ModelConfig,
  type RuleRecord,
  type SourceDocument,
} from "@/lib/ipc";
import { StylePage } from "@/features/style/StylePage";
import { useAppStore } from "@/stores/ui";

type Tab = "guide" | "model" | "import" | "style" | "rules" | "export";

export function CanonPage({ model }: { model: ModelConfig | null }) {
  const [tab, setTab] = useState<Tab>(() => {
    if (typeof window !== "undefined" && window.location.hash === "#guide") {
      window.history.replaceState(null, "", window.location.pathname + window.location.search);
      return "guide";
    }
    return "model";
  });
  return (
    <div className="fade-in mx-auto max-w-[1060px] px-8 py-7">
      <SectionTitle eyebrow="Canon · 设定" title="设定与项目配置" />
      <div className="mb-5 flex items-center gap-1 border-b border-line">
        {[
          { key: "model" as const, label: "模型", icon: Plug },
          { key: "guide" as const, label: "作者守则", icon: ScrollText },
          { key: "import" as const, label: "旧稿导入", icon: FileText },
          { key: "style" as const, label: "文笔", icon: Paintbrush },
          { key: "rules" as const, label: "规则", icon: ShieldAlert },
          { key: "export" as const, label: "导出", icon: Download },
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
                <span className="tab-thread" aria-hidden />
              ) : null}
            </button>
          );
        })}
      </div>
      {tab === "model" ? <ModelPanel model={model} /> : null}
      {tab === "guide" ? <GuidePanel /> : null}
      {tab === "import" ? <ImportPanel /> : null}
      {tab === "style" ? <StylePage /> : null}
      {tab === "rules" ? (
        <RulesPanel />
      ) : null}
      {tab === "export" ? <ExportPanel /> : null}
    </div>
  );
}

const guideTemplate = `视角纪律：
- 第三人称限知，只跟随主角；不写其他角色的内心独白。

禁写内容：
- 不写穿越与系统面板；不用现实品牌名。

命名与称谓：
- 主角在叙述中称名不称姓；长辈对其称小名。

文风红线：
- 不做总结式抒情，情绪用动作与感官呈现。`;

function GuidePanel() {
  const notify = useAppStore((s) => s.notify);
  const setNav = useAppStore((s) => s.setNav);
  const setAgentDraft = useAppStore((s) => s.setAgentDraft);
  const [content, setContent] = useState("");
  const [saved, setSaved] = useState("");
  const [updatedAt, setUpdatedAt] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void ipc
      .projectGuide()
      .then((guide) => {
        setContent(guide?.content ?? "");
        setSaved(guide?.content ?? "");
        setUpdatedAt(guide?.updatedAt ?? null);
      })
      .catch((cause) => setError(cause instanceof Error ? cause.message : String(cause)));
  }, []);

  const save = async () => {
    setBusy(true);
    setError(null);
    try {
      const guide = await ipc.projectGuideSave(content);
      setSaved(guide.content);
      setContent(guide.content);
      setUpdatedAt(guide.updatedAt);
      notify("作者守则已保存，Agent 下一轮起生效");
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setBusy(false);
    }
  };

  const dirty = content !== saved;
  return (
    <section className="paper-card max-w-[820px] p-6">
      <div className="flex items-start justify-between gap-4">
        <div>
          <h2 className="display text-[19px] text-ink">作者守则</h2>
          <p className="mt-1.5 max-w-xl text-[12px] leading-6 text-muted">
            写给 Agent 的长期规则：每次对话、规划和起草都会先读这里。它约束“怎么写”，不是世界事实；设定请放进人物与知识库。
          </p>
        </div>
        <Button
          size="sm"
          onClick={() => {
            setAgentDraft("根据种子卡和已确认设定，起草一份作者守则：视角纪律、禁写内容、命名约定和文风红线，各 2 至 4 条，用 propose_guide 提交候选。");
            setNav("agent");
          }}
        >
          让 Agent 起草
        </Button>
      </div>
      <textarea
        value={content}
        onChange={(event) => setContent(event.target.value)}
        placeholder={guideTemplate}
        rows={16}
        className="focus-ring mt-4 w-full resize-y rounded-xl border border-line-strong bg-paper-raised px-4 py-3 text-[13px] leading-7 text-ink placeholder:text-muted/60"
      />
      <div className="mt-3 flex items-center justify-between text-[11px] text-muted">
        <span>
          {content.length} 字
          {updatedAt ? ` · 上次保存 ${new Date(updatedAt).toLocaleString()}` : " · 尚未保存"}
        </span>
        <div className="flex items-center gap-2">
          {!content.trim() ? (
            <Button size="sm" tone="ghost" onClick={() => setContent(guideTemplate)}>
              使用模板
            </Button>
          ) : null}
          <Button size="sm" tone="primary" onClick={() => void save()} disabled={busy || !dirty}>
            {dirty ? "保存守则" : "已保存"}
          </Button>
        </div>
      </div>
      {error ? <div className="mt-2 text-[11px] text-coral">{error}</div> : null}
    </section>
  );
}

function ExportPanel() {
  const [includeWiki, setIncludeWiki] = useState(true);
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<ExportResult | null>(null);
  const [error, setError] = useState<string | null>(null);

  const run = async (format: "markdown" | "docx") => {
    setBusy(true);
    setError(null);
    try {
      setResult(await ipc.exportProject({ format, includeWiki }));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="paper-card max-w-[760px] p-6">
      <div className="flex items-start justify-between gap-6">
        <div>
          <div className="flex items-center gap-2 text-[14px] font-semibold text-ink">
            <Download size={15} className="text-teal" />
            本地项目导出
          </div>
          <p className="mt-1.5 max-w-xl text-[11.5px] leading-6 text-muted">
            从 SQLite 中的已提交章节生成 Markdown 或 DOCX。文件写入项目目录的 exports/；不会包含 API Key。
          </p>
        </div>
        <Badge tone="teal">本地文件</Badge>
      </div>
      <label className="mt-5 flex items-start gap-2 rounded-lg border border-line bg-paper/60 px-3 py-2.5">
        <input
          type="checkbox"
          checked={includeWiki}
          onChange={(e) => setIncludeWiki(e.target.checked)}
          className="mt-0.5 accent-teal"
        />
        <span className="text-[10.5px] leading-5 text-ink-soft">
          附加 Narrative Wiki 派生设定页
          <span className="block text-[9px] text-muted">
            附录会明确标记“非权威事实”，并保留来源 ID。
          </span>
        </span>
      </label>
      <div className="mt-5 flex gap-2">
        <Button
          tone="primary"
          onClick={() => void run("markdown")}
          disabled={busy}
          icon={busy ? <LoaderCircle size={12} className="animate-spin" /> : <FileText size={12} />}
        >
          导出 Markdown
        </Button>
        <Button
          onClick={() => void run("docx")}
          disabled={busy}
          icon={busy ? <LoaderCircle size={12} className="animate-spin" /> : <BookMarked size={12} />}
        >
          导出 DOCX
        </Button>
      </div>
      {error ? (
        <div className="mt-4 rounded-lg border border-coral/20 bg-coral-soft/50 px-3.5 py-2.5 text-[10.5px] text-coral">
          {error}
        </div>
      ) : null}
      {result ? (
        <div className="mt-4 rounded-lg border border-teal/20 bg-teal-soft/40 px-3.5 py-3">
          <div className="text-[10px] font-medium text-teal">
            已导出 {result.chapters} 章 · {result.words} 字 · {result.format.toUpperCase()}
          </div>
          <div className="mt-1 break-all font-mono text-[9px] leading-5 text-ink-soft">
            {result.path}
          </div>
          {result.includesWiki ? (
            <div className="mt-1 text-[9px] text-muted">含派生设定附录，不替代正文与权威事实。</div>
          ) : null}
        </div>
      ) : null}
    </section>
  );
}

function RulesPanel() {
  const [rules, setRules] = useState<RuleRecord[]>([]);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      setRules(await ipc.rules());
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const run = async () => {
    try {
      const result = await ipc.auditRun();
      setMessage(`审计完成，新增 ${result.issuesCreated} 个问题`);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  return (
    <section className="paper-card p-6">
      <div className="flex items-start justify-between">
        <div>
          <div className="flex items-center gap-2 text-[14px] font-semibold text-ink">
            <ShieldAlert size={15} className="text-teal" />
            规则编译器
          </div>
          <p className="mt-1.5 max-w-2xl text-[11.5px] leading-6 text-muted">
            普通规则走 SQL 快速检查；时间先后、同时两地、物品生命周期等硬约束交给内置时间约束求解器，冲突核心会映射回事实和原文证据。
          </p>
        </div>
        <Button size="sm" tone="primary" onClick={() => void run()}>
          运行全量审计
        </Button>
      </div>
      {message ? (
        <div className="mt-4 rounded-lg border border-teal/20 bg-teal-soft/50 px-3.5 py-2 text-[11px] text-teal">
          {message}
        </div>
      ) : null}
      {error ? (
        <div className="mt-4 rounded-lg border border-coral/20 bg-coral-soft/50 px-3.5 py-2 text-[11px] text-coral">
          {error}
        </div>
      ) : null}
      <div className="mt-5 space-y-2">
        {rules.map((rule) => (
          <div
            key={rule.id}
            className="flex items-center justify-between rounded-lg border border-line bg-paper/60 px-3.5 py-3"
          >
            <div>
              <div className="flex items-center gap-2">
                <span className="font-mono text-[10px] text-ink">{rule.code}</span>
                <Badge tone={rule.constraintLayer === "hard" ? "coral" : "neutral"}>
                  {rule.constraintLayer}
                </Badge>
                <Badge tone={rule.severity === "block" ? "coral" : "amber"}>
                  {rule.severity}
                </Badge>
              </div>
              <div className="mt-1 text-[10.5px] text-muted">{rule.description}</div>
            </div>
            <span className={`text-[9.5px] ${rule.enabled ? "text-teal" : "text-muted"}`}>
              {rule.enabled ? "启用" : "停用"}
            </span>
          </div>
        ))}
        {!rules.length ? (
          <div className="py-8 text-center text-[11px] text-muted">没有规则</div>
        ) : null}
      </div>
    </section>
  );
}

function ModelPanel({ model }: { model: ModelConfig | null }) {
  const setModel = useAppStore((s) => s.setModel);
  const project = useAppStore((s) => s.project);
  const [baseUrl, setBaseUrl] = useState(model?.baseUrl ?? "https://api.openai.com/v1");
  const [modelName, setModelName] = useState(model?.model ?? "gpt-4o-mini");
  const [maxTokens, setMaxTokens] = useState(model?.maxTokens ?? 4096);
  const [embeddingMode, setEmbeddingMode] = useState(model?.embeddingMode ?? "off");
  const [embeddingModel, setEmbeddingModel] = useState(model?.embeddingModel ?? "");
  const [embeddingBaseUrl, setEmbeddingBaseUrl] = useState(model?.embeddingBaseUrl ?? "");
  const [embeddingDimensions, setEmbeddingDimensions] = useState(
    model?.embeddingDimensions?.toString() ?? "",
  );
  const [embeddingBatchSize, setEmbeddingBatchSize] = useState(
    model?.embeddingBatchSize ?? 32,
  );
  const [embeddingAutoIndex, setEmbeddingAutoIndex] = useState(
    model?.embeddingAutoIndex ?? false,
  );
  const [embeddingStatus, setEmbeddingStatus] = useState<EmbeddingStatus | null>(null);
  const [embeddingProgress, setEmbeddingProgress] = useState<string | null>(null);
  const [indexing, setIndexing] = useState(false);
  const [apiKey, setApiKey] = useState("");
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<{ ok: boolean; text: string } | null>(null);

  const refreshEmbeddingStatus = useCallback(async () => {
    if (!project) return;
    try {
      setEmbeddingStatus(await ipc.embeddingStatus());
    } catch {
      setEmbeddingStatus(null);
    }
  }, [project]);

  useEffect(() => {
    void refreshEmbeddingStatus();
  }, [refreshEmbeddingStatus]);

  useEffect(() => {
    let dispose = () => {};
    void onEmbeddingProgress((event) => {
      setEmbeddingProgress(`${event.message} · ${event.model}`);
    }).then((unlisten) => {
      dispose = unlisten;
    });
    return () => dispose();
  }, []);

  const save = async () => {
    setBusy(true);
    try {
      const saved = await ipc.modelSave({
        baseUrl,
        model: modelName,
        temperature: model?.temperature ?? 0.8,
        maxTokens,
        embeddingMode,
        embeddingModel: embeddingModel.trim() || null,
        embeddingBaseUrl: embeddingBaseUrl.trim() || null,
        embeddingDimensions: embeddingDimensions.trim()
          ? Math.max(1, Number(embeddingDimensions))
          : null,
        embeddingBatchSize,
        embeddingAutoIndex,
        apiKey: apiKey.trim() || null,
      });
      setModel(saved);
      setApiKey("");
      setMessage({ ok: true, text: "模型与检索配置已保存" });
      await refreshEmbeddingStatus();
    } catch (e) {
      setMessage({ ok: false, text: e instanceof Error ? e.message : String(e) });
    } finally {
      setBusy(false);
    }
  };

  const testEmbedding = async () => {
    setBusy(true);
    try {
      await save();
      const result = await ipc.embeddingTest();
      setMessage({ ok: result.ok, text: `${result.message}（${result.latencyMs} ms）` });
    } catch (e) {
      setMessage({ ok: false, text: e instanceof Error ? e.message : String(e) });
    } finally {
      setBusy(false);
    }
  };

  const indexEmbeddings = async (rebuild: boolean) => {
    setIndexing(true);
    setEmbeddingProgress(rebuild ? "正在重建全部向量" : "正在检查失效与缺失向量");
    try {
      await save();
      const result = await ipc.embeddingIndex(rebuild);
      setMessage({
        ok: true,
        text: result.canceled
          ? `索引已取消，已提交 ${result.indexed} 条，剩余 ${result.remaining} 条`
          : `索引完成：新增 ${result.indexed} 条，复用 ${result.skipped} 条，剩余 ${result.remaining} 条`,
      });
      await refreshEmbeddingStatus();
    } catch (e) {
      setMessage({ ok: false, text: e instanceof Error ? e.message : String(e) });
    } finally {
      setIndexing(false);
      setEmbeddingProgress(null);
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
    <section className="paper-card max-w-[780px] p-6">
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
        <Field label="Max tokens" hint="结构化生成建议至少 4096">
          <Input
            type="number"
            min={64}
            max={200000}
            value={maxTokens}
            onChange={(e) => setMaxTokens(Number(e.target.value))}
          />
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

      <div className="my-5 h-px bg-line" />
      <div className="mb-4 flex items-start justify-between gap-4">
        <div>
          <div className="text-[13px] font-semibold text-ink">可选 Embedding</div>
          <div className="mt-1 text-[10.5px] leading-5 text-muted">
            关闭时使用 FTS5 + PPR。开启后按内容哈希增量更新，并用 RRF 融合三类召回。
          </div>
        </div>
        <Badge
          tone={
            embeddingMode === "off"
              ? "neutral"
              : embeddingStatus?.ready
                ? "teal"
                : "amber"
          }
        >
          {embeddingMode === "off"
            ? "已关闭"
            : `${embeddingStatus?.ready ?? 0} 条就绪`}
        </Badge>
      </div>
      <div className="grid grid-cols-2 gap-3">
        <Field label="Embedding 模式">
          <select
            value={embeddingMode}
            onChange={(event) => setEmbeddingMode(event.target.value)}
            className="focus-ring h-10 w-full rounded-[10px] border border-line-strong bg-paper-raised px-3 text-[12px] text-ink"
          >
            <option value="off">关闭</option>
            <option value="local">本地 OpenAI 兼容服务</option>
            <option value="api">OpenAI 兼容 Embedding API</option>
          </select>
        </Field>
        <Field label="Embedding 模型">
          <Input
            value={embeddingModel}
            onChange={(event) => setEmbeddingModel(event.target.value)}
            placeholder="text-embedding-3-small"
            disabled={embeddingMode === "off"}
          />
        </Field>
        <Field label="Embedding Base URL" hint="留空使用上方 Base URL">
          <Input
            value={embeddingBaseUrl}
            onChange={(event) => setEmbeddingBaseUrl(event.target.value)}
            placeholder="http://127.0.0.1:11434/v1"
            disabled={embeddingMode === "off"}
          />
        </Field>
        <div className="grid grid-cols-2 gap-3">
          <Field label="向量维度" hint="可选">
            <Input
              type="number"
              min={1}
              value={embeddingDimensions}
              onChange={(event) => setEmbeddingDimensions(event.target.value)}
              placeholder="由服务决定"
              disabled={embeddingMode === "off"}
            />
          </Field>
          <Field label="批大小">
            <Input
              type="number"
              min={1}
              max={256}
              value={embeddingBatchSize}
              onChange={(event) => setEmbeddingBatchSize(Number(event.target.value))}
              disabled={embeddingMode === "off"}
            />
          </Field>
        </div>
      </div>
      <label className="mt-3 flex items-center gap-2 text-[10.5px] text-ink-soft">
        <input
          type="checkbox"
          checked={embeddingAutoIndex}
          onChange={(event) => setEmbeddingAutoIndex(event.target.checked)}
          disabled={embeddingMode === "off"}
          className="h-3.5 w-3.5 accent-teal"
        />
        项目打开后自动增量补建失效向量
      </label>
      <div className="mt-4 rounded-lg bg-paper px-3.5 py-3">
        <div className="flex flex-wrap items-center gap-x-4 gap-y-1 text-[9.5px] text-muted">
          <span>就绪 {embeddingStatus?.ready ?? 0}</span>
          <span>失效 {embeddingStatus?.stale ?? 0}</span>
          <span>失败 {embeddingStatus?.error ?? 0}</span>
          <span>其他模型 {embeddingStatus?.otherModel ?? 0}</span>
        </div>
        {embeddingProgress ? (
          <div className="mt-2 flex items-center gap-1.5 text-[9.5px] text-teal">
            <LoaderCircle size={10} className="animate-spin" />
            {embeddingProgress}
          </div>
        ) : null}
      </div>
      <div className="mt-3 flex flex-wrap items-center justify-between gap-2">
        <div className="max-w-[410px] text-[9.5px] leading-4 text-muted">
          开启 API 模式会把选中的正文摘要、人物、知识、事实、伏笔和 Wiki 内容发送给 Embedding 服务。API Key
          仍只保存在系统钥匙串，不进入项目或导出。
        </div>
        <div className="flex gap-2">
          {indexing ? (
            <Button
              size="sm"
              tone="ghost"
              onClick={() => void ipc.embeddingCancelIndex()}
            >
              取消索引
            </Button>
          ) : null}
          <Button
            size="sm"
            onClick={() => void testEmbedding()}
            disabled={busy || indexing || embeddingMode === "off"}
          >
            测试 Embedding
          </Button>
          <Button
            size="sm"
            onClick={() => void indexEmbeddings(false)}
            disabled={busy || indexing || embeddingMode === "off"}
          >
            增量更新
          </Button>
          <Button
            size="sm"
            tone="primary"
            onClick={() => void indexEmbeddings(true)}
            disabled={busy || indexing || embeddingMode === "off"}
          >
            重建索引
          </Button>
        </div>
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
