import leafSprig from "@/assets/leaf-sprig.png";
import { BrandMark } from "@/components/BrandMark";
import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import {
  ArrowRight,
  Check,
  CircleAlert,
  FolderOpen,
  Layers,
  LoaderCircle,
  Sparkles,
  Unplug,
  Wand2,
} from "lucide-react";
import { Button, Field, Input } from "@/components/ui";
import { ipc, isTauri, type ModelConfig, type ProjectSnapshot } from "@/lib/ipc";

type Props = {
  model: ModelConfig | null;
  onProject: (project: ProjectSnapshot) => void;
  onModel: (model: ModelConfig) => void;
};

const modes = [
  {
    id: "idea",
    title: "从一句话开始",
    body: "用一个画面或冲突孵化种子卡，再按作者需要规划章节或卷章。",
    icon: Sparkles,
  },
  {
    id: "import",
    title: "导入旧稿",
    body: "导入已有正文，抽取人物、事件和待确认事实。",
    icon: FolderOpen,
  },
  {
    id: "both",
    title: "两者都有",
    body: "带稿起步，同时补全新故事的核心契约。",
    icon: Layers,
  },
];

export function Onboarding({ model, onProject, onModel }: Props) {
  const [tab, setTab] = useState<"create" | "open">("create");
  const [name, setName] = useState("");
  const [rootPath, setRootPath] = useState("");
  const [mode, setMode] = useState("idea");
  const [baseUrl, setBaseUrl] = useState(model?.baseUrl ?? "https://api.openai.com/v1");
  const [modelName, setModelName] = useState(model?.model ?? "gpt-4o-mini");
  const [apiKey, setApiKey] = useState("");
  const [testing, setTesting] = useState(false);
  const [testMessage, setTestMessage] = useState<{ ok: boolean; text: string } | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const chooseDirectory = async () => {
    if (!isTauri()) {
      setError("浏览器预览不能调用系统目录选择器，请直接填写路径，或使用桌面应用。");
      return;
    }
    const selected = await open({ directory: true, multiple: false, title: "选择项目目录" });
    if (typeof selected === "string") setRootPath(selected);
  };

  const saveModel = async () => {
    const saved = await ipc.modelSave({
      baseUrl,
      model: modelName,
      temperature: model?.temperature ?? 0.8,
      maxTokens: model?.maxTokens ?? 4096,
      embeddingMode: model?.embeddingMode ?? "off",
      embeddingModel: model?.embeddingModel ?? null,
      embeddingBaseUrl: model?.embeddingBaseUrl ?? null,
      embeddingDimensions: model?.embeddingDimensions ?? null,
      embeddingBatchSize: model?.embeddingBatchSize ?? 32,
      embeddingAutoIndex: model?.embeddingAutoIndex ?? false,
      apiKey: apiKey.trim() || null,
    });
    onModel(saved);
    setApiKey("");
    return saved;
  };

  const testModel = async () => {
    setTesting(true);
    setTestMessage(null);
    try {
      await saveModel();
      const result = await ipc.modelTest();
      setTestMessage({ ok: result.ok, text: result.message });
    } catch (e) {
      setTestMessage({ ok: false, text: e instanceof Error ? e.message : String(e) });
    } finally {
      setTesting(false);
    }
  };

  const create = async () => {
    setBusy(true);
    setError(null);
    try {
      await saveModel();
      const project = await ipc.createProject({
        name,
        rootPath,
        mode,
        writingLanguage: "zh-CN",
      });
      onProject(project);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const openProject = async () => {
    setBusy(true);
    setError(null);
    try {
      const project = await ipc.openProject(rootPath);
      onProject(project);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const canCreate = name.trim().length > 0 && rootPath.trim().length > 0 && !busy;

  return (
    <div className="flex h-full min-h-0">
      <aside className="relative hidden w-[46%] min-w-[480px] flex-col justify-between overflow-hidden border-r border-line px-14 pb-12 pt-16 lg:flex">
        <div
          aria-hidden
          className="pointer-events-none absolute -right-10 bottom-[-40px] h-[560px] w-[372px] bg-contain bg-no-repeat opacity-80"
          style={{ backgroundImage: `url(${leafSprig})` }}
        />
        <div className="relative">
          <div className="mb-16 flex items-center gap-3">
            <BrandMark size={40} />
            <div>
              <div className="text-[15px] font-semibold tracking-wide text-ink">CanonWeave</div>
              <div className="text-[11px] tracking-[0.2em] text-muted">叙事织网</div>
            </div>
          </div>
          <h1 className="display max-w-[480px] text-[52px] leading-[1.16] text-ink">
            让每一个故事
            <br />
            <span className="text-teal">保持完整。</span>
          </h1>
          <p className="mt-7 max-w-[430px] text-[14px] leading-8 text-ink-soft">
            CanonWeave 记录故事世界中的事实、认知与时间，把人物、关系与伏笔编织成一张可追溯的叙事网。
          </p>
          <div className="mt-12 space-y-5">
            {[
              ["多视角真值", "世界事实、角色认知与不可靠叙述分开记录"],
              ["证据链", "每条冲突与事实都能回到原文"],
              ["可控文笔", "项目级文笔档案，快速与精修双通道"],
            ].map(([title, body], index) => (
              <div key={title} className="flex gap-4">
                <div className="mt-2 h-2 w-2 shrink-0 rotate-45 rounded-[1px_6px] bg-silk" aria-hidden data-index={index} />
                <div>
                  <div className="text-[13px] font-medium text-ink">{title}</div>
                  <div className="mt-0.5 text-[12px] leading-6 text-muted">{body}</div>
                </div>
              </div>
            ))}
          </div>
        </div>
        <div className="relative text-[11px] tracking-wide text-muted">
          本地优先 · 项目数据只保存在你选择的目录中
        </div>
      </aside>

      <main className="flex min-w-0 flex-1 items-center justify-center overflow-y-auto px-8 py-10">
        <div className="fade-in w-full max-w-[540px]">
          <div className="mb-7 flex items-center justify-between">
            <div>
              <h2 className="display text-[26px] text-ink">
                {tab === "create" ? "建立你的故事工作台" : "打开已有项目"}
              </h2>
            </div>
            <div className="flex rounded-full border border-line bg-paper-raised p-0.5 text-[11px]">
              <button
                onClick={() => setTab("create")}
                className={`rounded-full px-3 py-1.5 transition ${tab === "create" ? "bg-teal text-paper-raised" : "text-muted hover:text-ink"}`}
              >
                新建
              </button>
              <button
                onClick={() => setTab("open")}
                className={`rounded-full px-3 py-1.5 transition ${tab === "open" ? "bg-teal text-paper-raised" : "text-muted hover:text-ink"}`}
              >
                打开
              </button>
            </div>
          </div>

          <section className="paper-card mb-4 p-5">
            <div className="mb-4 flex items-center justify-between">
              <div className="flex items-center gap-2 text-[13px] font-semibold text-ink">
                <Unplug size={15} className="text-teal" />
                模型连接
                {model?.hasApiKey ? <StatusDot ok /> : null}
              </div>
              <span className="text-[11px] text-muted">OpenAI 兼容接口</span>
            </div>
            <div className="grid grid-cols-2 gap-3">
              <Field label="Base URL">
                <Input
                  value={baseUrl}
                  onChange={(e) => setBaseUrl(e.target.value)}
                  placeholder="https://api.openai.com/v1"
                />
              </Field>
              <Field label="模型">
                <Input value={modelName} onChange={(e) => setModelName(e.target.value)} />
              </Field>
            </div>
            <div className="mt-3">
              <Field label="API Key" hint="保存到系统钥匙串，不写入项目">
                <Input
                  type="password"
                  value={apiKey}
                  onChange={(e) => setApiKey(e.target.value)}
                  placeholder={model?.hasApiKey ? "已保存，留空则保持不变" : "sk-..."}
                  autoComplete="off"
                />
              </Field>
            </div>
            <div className="mt-3 flex items-center justify-between gap-3">
              <div className="min-h-[18px] flex-1 text-[11px] leading-5">
                {testMessage ? (
                  <span className={testMessage.ok ? "text-teal" : "text-coral"}>
                    {testMessage.ok ? "连接成功 · " : ""}
                    {testMessage.text}
                  </span>
                ) : (
                  <span className="text-muted">可先创建项目，稍后在设置中测试</span>
                )}
              </div>
              <Button size="sm" tone="ghost" onClick={testModel} disabled={testing}>
                {testing ? <LoaderCircle size={13} className="animate-spin" /> : null}
                测试连接
              </Button>
            </div>
          </section>

          {tab === "open" ? (
            <section className="paper-card p-5">
              <Field label="项目目录" hint="包含 project.json 的目录">
                <div className="flex gap-2">
                  <Input
                    value={rootPath}
                    onChange={(e) => setRootPath(e.target.value)}
                    placeholder="/Users/you/Stories/雨夜"
                  />
                  <Button tone="secondary" onClick={chooseDirectory} icon={<FolderOpen size={14} />}>
                    选择
                  </Button>
                </div>
              </Field>
              {error ? <ErrorLine text={error} /> : null}
              <Button
                tone="primary"
                className="mt-5 w-full"
                disabled={!rootPath.trim() || busy}
                onClick={openProject}
                icon={<ArrowRight size={15} />}
              >
                打开项目
              </Button>
            </section>
          ) : (
            <section className="paper-card p-5">
              <div className="mb-4 flex items-center gap-2 text-[13px] font-semibold text-ink">
                <Wand2 size={15} className="text-teal" />
                项目起点
              </div>
              <div className="grid grid-cols-3 gap-2">
                {modes.map((item) => {
                  const Icon = item.icon;
                  const active = mode === item.id;
                  return (
                    <button
                      key={item.id}
                      onClick={() => setMode(item.id)}
                      className={`focus-ring rounded-xl border p-3 text-left transition ${
                        active
                          ? "border-teal/60 bg-teal-soft/70"
                          : "border-line bg-paper-raised hover:border-line-strong"
                      }`}
                    >
                      <div className="mb-2 flex items-center justify-between">
                        <Icon size={15} className={active ? "text-teal" : "text-muted"} />
                        {active ? <Check size={13} className="text-teal" /> : null}
                      </div>
                      <div className="text-[12px] font-medium text-ink">{item.title}</div>
                      <div className="mt-1 text-[10.5px] leading-4 text-muted">{item.body}</div>
                    </button>
                  );
                })}
              </div>

              <div className="mt-4 grid grid-cols-2 gap-3">
                <Field label="项目名称">
                  <Input
                    value={name}
                    onChange={(e) => setName(e.target.value)}
                    placeholder="例如：雨夜的第七封信"
                  />
                </Field>
                <Field label="项目目录">
                  <div className="flex gap-2">
                    <Input
                      value={rootPath}
                      onChange={(e) => setRootPath(e.target.value)}
                      placeholder="选择空目录"
                    />
                    <Button size="md" tone="secondary" onClick={chooseDirectory} icon={<FolderOpen size={14} />}>
                      <span className="sr-only">选择目录</span>
                    </Button>
                  </div>
                </Field>
              </div>
              {error ? <ErrorLine text={error} /> : null}
              <Button
                tone="primary"
                className="mt-5 w-full"
                disabled={!canCreate}
                onClick={create}
                icon={<ArrowRight size={15} />}
              >
                {busy ? "正在建立…" : "创建项目并进入"}
              </Button>
            </section>
          )}

          <p className="mt-5 text-center text-[11px] leading-5 text-muted">
            API Key 仅用于你配置的模型服务。项目目录中的事实、版本和导出不包含密钥。
          </p>
        </div>
      </main>
    </div>
  );
}

function StatusDot({ ok }: { ok: boolean }) {
  return (
    <span
      className={`ml-1 inline-block h-1.5 w-1.5 rounded-full ${ok ? "bg-teal" : "bg-amber"}`}
      title={ok ? "已保存密钥" : "未保存密钥"}
    />
  );
}

function ErrorLine({ text }: { text: string }) {
  return (
    <div className="mt-3 flex items-start gap-2 rounded-lg border border-coral/20 bg-coral-soft/60 px-3 py-2 text-[11px] leading-5 text-coral">
      <CircleAlert size={13} className="mt-0.5 shrink-0" />
      {text}
    </div>
  );
}
