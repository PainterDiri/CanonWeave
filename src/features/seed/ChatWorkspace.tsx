import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import * as DropdownMenu from "@radix-ui/react-dropdown-menu";
import {
  Archive,
  ArrowUp,
  Copy,
  CornerDownLeft,
  LoaderCircle,
  MessageSquarePlus,
  Pin,
  Square,
  Trash2,
} from "lucide-react";
import { Badge, Button, Textarea } from "@/components/ui";
import {
  ipc,
  onChatDelta,
  type BeatOption,
  type Candidate,
  type ChapterRecord,
  type ChatMessage,
  type ChatSession,
  type Premise,
} from "@/lib/ipc";
import { useAppStore } from "@/stores/ui";

const modes = [
  { key: "diverge", label: "发散" },
  { key: "structure", label: "结构" },
  { key: "plot", label: "推进剧情" },
  { key: "lookup", label: "查设定" },
];

const captureKinds = [
  { key: "character", label: "人物" },
  { key: "setting", label: "设定" },
  { key: "place", label: "地点" },
  { key: "rule", label: "规则" },
  { key: "promise", label: "伏笔" },
  { key: "scene_goal", label: "章节目标" },
  { key: "style_sample", label: "文笔样本" },
];

export function ChatWorkspace({ premise }: { premise: Premise | null }) {
  const [sessions, setSessions] = useState<ChatSession[]>([]);
  const [active, setActive] = useState<ChatSession | null>(null);
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [candidates, setCandidates] = useState<Candidate[]>([]);
  const [chapters, setChapters] = useState<ChapterRecord[]>([]);
  const [chapterId, setChapterId] = useState<string>("");
  const [beats, setBeats] = useState<BeatOption[]>([]);
  const [beatInstruction, setBeatInstruction] = useState("");
  const [beatBusy, setBeatBusy] = useState(false);
  const [input, setInput] = useState("");
  const [streaming, setStreaming] = useState("");
  const [sending, setSending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const requestRef = useRef<string | null>(null);
  const scrollRef = useRef<HTMLDivElement>(null);
  const refreshProject = useAppStore((s) => s.refreshProject);
  const notify = useAppStore((s) => s.notify);

  const loadSessions = useCallback(async (selectId?: string) => {
    const list = await ipc.chatSessions(null);
    setSessions(list);
    setActive((current) => {
      const next =
        list.find((session) => session.id === (selectId ?? current?.id)) ?? list[0] ?? null;
      return next;
    });
  }, []);

  const loadActive = useCallback(async (session: ChatSession) => {
    const [history, pending] = await Promise.all([
      ipc.chatMessages(session.id),
      ipc.chatCandidates(session.id),
    ]);
    setMessages(history);
    setCandidates(pending.filter((c) => c.status === "pending"));
  }, []);

  useEffect(() => {
    (async () => {
      try {
        const [list, chapterList] = await Promise.all([
          ipc.chatSessions(null),
          ipc.chapters().catch(() => []),
        ]);
        setChapters(chapterList);
        setChapterId(chapterList[0]?.id ?? "");
        let first = list[0];
        if (!first) {
          first = await ipc.chatCreate({
            scope: "seed",
            mode: "diverge",
            title: "灵感孵化",
            sceneId: null,
            branchId: null,
            parentSessionId: null,
          });
        }
        setSessions(first ? [first, ...list.filter((s) => s.id !== first.id)] : list);
        setActive(first ?? null);
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
      } finally {
        setLoading(false);
      }
    })();
  }, []);

  useEffect(() => {
    if (!active) return;
    void loadActive(active).catch((e) =>
      setError(e instanceof Error ? e.message : String(e)),
    );
  }, [active, loadActive]);

  useEffect(() => {
    let dispose = () => {};
    void onChatDelta((event) => {
      if (event.sessionId !== active?.id) return;
      requestRef.current = event.requestId;
      setStreaming((prev) => prev + event.delta);
    }).then((unlisten) => {
      dispose = unlisten;
    });
    return () => dispose();
  }, [active?.id]);

  useEffect(() => {
    const node = scrollRef.current;
    if (node) node.scrollTop = node.scrollHeight;
  }, [messages, streaming]);

  const newSession = async () => {
    try {
      const scope = active?.mode === "plot" ? "plot" : active?.mode === "lookup" ? "lookup" : "seed";
      const session = await ipc.chatCreate({
        scope,
        mode: active?.mode ?? "diverge",
        title: null,
        sceneId: null,
        branchId: null,
        parentSessionId: null,
      });
      await loadSessions(session.id);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const send = async (preset?: string) => {
    const content = (preset ?? input).trim();
    if (!content || !active || sending) return;
    setInput("");
    setSending(true);
    setStreaming("");
    setError(null);
    requestRef.current = null;
    const temp: ChatMessage = {
      id: `temp-${Date.now()}`,
      sessionId: active.id,
      seq: 999999,
      role: "user",
      content,
      model: "",
      tokens: 0,
      createdAt: new Date().toISOString(),
      linkedCandidateIds: [],
      pinned: false,
    };
    setMessages((prev) => [...prev, temp]);
    try {
      const result = await ipc.chatSend(active.id, content);
      setMessages((prev) => [
        ...prev.filter((message) => message.id !== temp.id),
        result.userMessage,
        result.assistantMessage,
      ]);
      setActive(result.session);
      setStreaming("");
      await loadSessions(active.id);
      await refreshProject();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setMessages((prev) => prev.filter((message) => message.id !== temp.id));
      setStreaming("");
    } finally {
      setSending(false);
      requestRef.current = null;
    }
  };

  const cancel = async () => {
    if (requestRef.current) await ipc.chatCancel(requestRef.current);
  };

  const updateMode = async (mode: string) => {
    if (!active) return;
    try {
      const session = await ipc.chatUpdate(active.id, {
        mode,
        scope: mode === "plot" ? "plot" : mode === "lookup" ? "lookup" : active.scope,
      });
      setActive(session);
      await loadSessions(session.id);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const capture = async (message: ChatMessage, kind: string) => {
    if (!active) return;
    try {
      await ipc.chatCapture({
        sessionId: active.id,
        messageId: message.id,
        kind,
        text: message.content,
      });
      const pending = await ipc.chatCandidates(active.id);
      setCandidates(pending.filter((c) => c.status === "pending"));
      await refreshProject();
      notify("已加入候选区");
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const acceptCandidate = async (id: string) => {
    if (!active) return;
    try {
      await ipc.candidateAccept(id);
      const pending = await ipc.chatCandidates(active.id);
      setCandidates(pending.filter((c) => c.status === "pending"));
      await refreshProject();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const togglePin = async () => {
    if (!active) return;
    const session = await ipc.chatUpdate(active.id, { pinned: !active.pinned });
    setActive(session);
    await loadSessions(session.id);
  };

  const fork = async () => {
    if (!active) return;
    const session = await ipc.chatFork(active.id);
    await loadSessions(session.id);
  };

  const archiveSession = async () => {
    if (!active) return;
    await ipc.chatArchive(active.id);
    await loadSessions();
  };

  const removeSession = async () => {
    if (!active) return;
    if (!window.confirm("删除这个会话？消息会被删除，但已确认事实与候选不会受影响。")) return;
    await ipc.chatDelete(active.id);
    await loadSessions();
  };

  const generateBeats = async () => {
    if (!active || !beatInstruction.trim()) return;
    setBeatBusy(true);
    setError(null);
    try {
      const result = await ipc.plotBeats(active.id, beatInstruction.trim());
      setBeats(result);
      await loadActive(active);
      await refreshProject();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBeatBusy(false);
    }
  };

  const continueWrite = async () => {
    if (!active) return;
    setBeatBusy(true);
    try {
      await ipc.chatContinue(active.id);
      setCandidates(await ipc.chatCandidates(active.id));
      await refreshProject();
      notify("续写草稿已进入候选区（500–800 字）");
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBeatBusy(false);
    }
  };

  const toScene = async () => {
    if (!active || !chapterId) {
      setError("请先在骨架中确认章节，再写成场景");
      return;
    }
    setBeatBusy(true);
    try {
      await ipc.chatToScene(active.id, chapterId);
      notify("场景契约已生成，请到正文工坊查看");
      await refreshProject();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBeatBusy(false);
    }
  };

  if (loading) {
    return (
      <div className="flex h-64 items-center justify-center text-[12px] text-muted">
        <LoaderCircle size={15} className="mr-2 animate-spin" /> 正在打开创作会话
      </div>
    );
  }

  return (
    <div
      className="grid grid-cols-[210px_minmax(0,1fr)_280px] gap-3"
      style={{ height: "calc(100vh - 245px)", minHeight: 520 }}
    >
      <aside className="paper-card flex min-h-0 flex-col overflow-hidden">
        <div className="flex items-center justify-between border-b border-line px-3 py-2.5">
          <span className="text-[11px] font-semibold tracking-wide text-ink-soft">会话</span>
          <button
            onClick={() => void newSession()}
            className="focus-ring flex h-6 w-6 items-center justify-center rounded-md text-muted hover:bg-ink/5 hover:text-teal"
            title="新建会话"
          >
            <MessageSquarePlus size={14} />
          </button>
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto p-1.5">
          {sessions.map((session) => (
            <button
              key={session.id}
              onClick={() => setActive(session)}
              className={`group mb-1 w-full rounded-lg px-2.5 py-2 text-left transition ${
                active?.id === session.id ? "bg-teal-soft/80" : "hover:bg-ink/[0.035]"
              }`}
            >
              <div className="flex items-center gap-1.5">
                {session.pinned ? <Pin size={9} className="shrink-0 text-amber" /> : null}
                <span className="truncate text-[11.5px] font-medium text-ink">{session.title}</span>
              </div>
              <div className="mt-1 flex items-center gap-1.5 text-[9.5px] text-muted">
                <span>{scopeLabel(session.scope)}</span>
                <span className="h-0.5 w-0.5 rounded-full bg-muted/50" />
                <span>{modeLabel(session.mode)}</span>
                {session.pendingCandidates > 0 ? (
                  <span className="ml-auto rounded-full bg-amber-soft px-1.5 text-amber">
                    {session.pendingCandidates}
                  </span>
                ) : null}
              </div>
            </button>
          ))}
        </div>
        <div className="flex items-center justify-around border-t border-line px-1 py-1.5">
          <IconAction label={active?.pinned ? "取消置顶" : "置顶"} onClick={() => void togglePin()}>
            <Pin size={12} />
          </IconAction>
          <IconAction label="分叉会话" onClick={() => void fork()}>
            <Copy size={12} />
          </IconAction>
          <IconAction label="归档会话" onClick={() => void archiveSession()}>
            <Archive size={12} />
          </IconAction>
          <IconAction label="删除会话" onClick={() => void removeSession()}>
            <Trash2 size={12} />
          </IconAction>
        </div>
      </aside>

      <section className="paper-card flex min-h-0 flex-col overflow-hidden">
        <div className="flex items-center justify-between border-b border-line px-4 py-2.5">
          <div className="flex items-center gap-3">
            <span className="max-w-[260px] truncate text-[12px] font-semibold text-ink">
              {active?.title ?? "创作对话"}
            </span>
            <div className="flex rounded-full border border-line p-0.5">
              {modes.map((mode) => (
                <button
                  key={mode.key}
                  onClick={() => void updateMode(mode.key)}
                  className={`rounded-full px-2 py-0.5 text-[9.5px] transition ${
                    active?.mode === mode.key
                      ? "bg-teal text-paper-raised"
                      : "text-muted hover:text-ink"
                  }`}
                >
                  {mode.label}
                </button>
              ))}
            </div>
          </div>
          {premise ? <Badge tone="teal">种子卡已注入</Badge> : <Badge tone="amber">无种子卡</Badge>}
        </div>

        <div ref={scrollRef} className="min-h-0 flex-1 overflow-y-auto px-5 py-5">
          {messages.length === 0 && !streaming ? (
            <div className="flex h-full flex-col items-center justify-center text-center">
              <div className="mb-4 flex h-10 w-10 items-center justify-center rounded-full border border-teal/20 bg-teal-soft/40 text-teal">
                <CornerDownLeft size={16} />
              </div>
              <div className="display text-[18px] text-ink">对话只产出建议，不直接成为事实</div>
              <p className="mt-2 max-w-md text-[11.5px] leading-6 text-muted">
                讨论人物、规则或章节目标。AI 消息可以“采纳为”候选，确认后才进入正式表。
              </p>
              <div className="mt-5 flex flex-wrap justify-center gap-2">
                {[
                  "这个主角最大的秘密会怎样反噬他？",
                  "给我三个第一场可以发生的冲突。",
                  "这个世界规则有什么代价？",
                ].map((prompt) => (
                  <button
                    key={prompt}
                    onClick={() => void send(prompt)}
                    className="rounded-full border border-line bg-paper-raised px-3 py-1.5 text-[10.5px] text-ink-soft transition hover:border-teal/40 hover:text-teal"
                  >
                    {prompt}
                  </button>
                ))}
              </div>
            </div>
          ) : (
            <div className="space-y-5">
              {messages.map((message) => (
                <MessageBubble
                  key={message.id}
                  message={message}
                  onCapture={(kind) => void capture(message, kind)}
                />
              ))}
              {streaming ? (
                <div className="flex gap-3">
                  <div className="mt-1 flex h-6 w-6 shrink-0 items-center justify-center rounded-full bg-teal text-[9px] font-semibold text-paper-raised">
                    AI
                  </div>
                  <div className="max-w-[86%] rounded-xl rounded-tl-sm border border-line bg-paper-raised px-4 py-3 text-[12px] leading-6 whitespace-pre-wrap text-ink-soft">
                    {streaming}
                    <span className="ml-1 inline-block h-3 w-0.5 animate-pulse bg-teal align-middle" />
                  </div>
                </div>
              ) : null}
            </div>
          )}
        </div>

        <div className="border-t border-line px-4 py-3">
          {active?.mode === "plot" || beats.length > 0 ? (
            <div className="mb-2.5 rounded-lg border border-amber/20 bg-amber-soft/30 px-3 py-2.5">
              <div className="mb-2 flex items-center gap-2">
                <span className="text-[10px] font-semibold text-amber">推进剧情 · 先给节拍，不写整章</span>
                <input
                  value={beatInstruction}
                  onChange={(e) => setBeatInstruction(e.target.value)}
                  placeholder="下一场你想发生什么？"
                  className="h-7 flex-1 rounded-md border border-line bg-paper-raised px-2 text-[10.5px] outline-none"
                  onKeyDown={(e) => {
                    if (e.key === "Enter") void generateBeats();
                  }}
                />
                <Button size="sm" tone="ghost" onClick={() => void generateBeats()} disabled={beatBusy}>
                  {beatBusy ? <LoaderCircle size={11} className="animate-spin" /> : null}
                  三个节拍
                </Button>
              </div>
              {beats.length > 0 ? (
                <div className="grid grid-cols-3 gap-2">
                  {beats.map((beat, index) => (
                    <div key={beat.title} className="rounded-md border border-line bg-paper-raised p-2">
                      <div className="text-[9.5px] font-semibold text-ink">
                        {String.fromCharCode(65 + index)} · {beat.title}
                      </div>
                      <div className="mt-1 line-clamp-3 text-[9px] leading-4 text-muted">{beat.beat}</div>
                      <button
                        onClick={() =>
                          void ipc
                            .sceneFromBeat(chapterId, beat)
                            .then(() => notify("场景契约已建立，可在正文工坊查看"))
                            .catch((e: unknown) =>
                              setError(e instanceof Error ? e.message : String(e)),
                            )
                        }
                        disabled={!chapterId}
                        className="mt-1.5 text-[9px] text-teal hover:underline disabled:text-muted"
                      >
                        写成场景 →
                      </button>
                    </div>
                  ))}
                </div>
              ) : null}
            </div>
          ) : null}
          {error ? <div className="mb-2 text-[11px] leading-5 text-coral">{error}</div> : null}
          <div className="relative">
            <Textarea
              rows={3}
              value={input}
              onChange={(e) => setInput(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && !e.shiftKey) {
                  e.preventDefault();
                  void send();
                }
              }}
              placeholder="继续讨论，或让 AI 推演下一场…（Enter 发送，Shift+Enter 换行）"
              disabled={sending}
              className="pr-24"
            />
            <div className="absolute bottom-2.5 right-2.5 flex items-center gap-1.5">
              {sending ? (
                <Button size="sm" tone="ghost" onClick={() => void cancel()} icon={<Square size={11} />}>
                  停止
                </Button>
              ) : null}
              <Button
                size="sm"
                tone="primary"
                onClick={() => void send()}
                disabled={!input.trim() || sending || !active}
                icon={<ArrowUp size={13} />}
              >
                发送
              </Button>
            </div>
          </div>
          <div className="mt-2 flex items-center gap-3 text-[9.5px] text-muted">
            <span>对话不会自动写入正文或权威库</span>
            <span className="h-0.5 w-0.5 rounded-full bg-muted/50" />
            <span>超过 12 轮建议先写第一场或生成骨架</span>
            <span className="ml-auto flex items-center gap-1.5">
              <select
                value={chapterId}
                onChange={(e) => setChapterId(e.target.value)}
                className="rounded border border-line bg-paper-raised px-1.5 py-0.5 text-[9px]"
              >
                <option value="">选择章节</option>
                {chapters.map((chapter) => (
                  <option key={chapter.id} value={chapter.id}>
                    第 {chapter.number} 章 {chapter.title}
                  </option>
                ))}
              </select>
              <button
                onClick={() => void continueWrite()}
                disabled={beatBusy}
                className="rounded border border-line px-1.5 py-0.5 hover:border-teal/40 hover:text-teal disabled:opacity-50"
              >
                继续写 500–800 字
              </button>
              <button
                onClick={() => void toScene()}
                disabled={beatBusy || !chapterId}
                className="rounded border border-line px-1.5 py-0.5 hover:border-teal/40 hover:text-teal disabled:opacity-50"
              >
                写成场景
              </button>
            </span>
          </div>
        </div>
      </section>

      <aside className="paper-card flex min-h-0 flex-col overflow-hidden">
        <div className="flex items-center justify-between border-b border-line px-3.5 py-2.5">
          <div>
            <div className="text-[11px] font-semibold text-ink-soft">候选区</div>
            <div className="text-[9.5px] text-muted">采纳后仍需确认</div>
          </div>
          <span className="flex h-5 min-w-5 items-center justify-center rounded-full bg-amber-soft px-1.5 text-[9.5px] font-semibold text-amber">
            {candidates.length}
          </span>
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto p-2.5">
          {candidates.length === 0 ? (
            <div className="px-2 py-8 text-center text-[10.5px] leading-5 text-muted">
              在 AI 消息下选择“采纳为”，候选会出现在这里。
            </div>
          ) : (
            <div className="space-y-2">
              {candidates.map((candidate) => (
                <div
                  key={candidate.id}
                  className="rounded-xl border border-dashed border-line-strong bg-paper/60 p-3"
                >
                  <div className="mb-1.5 flex items-center justify-between">
                    <span className="text-[10px] font-semibold text-teal">
                      {candidateLabel(candidate.kind)}
                    </span>
                    <span className="rounded bg-ink/5 px-1.5 py-0.5 text-[9px] text-muted">
                      {candidate.riskLevel}
                    </span>
                  </div>
                  <div className="text-[10.5px] leading-5 text-ink-soft">
                    {summaryOf(candidate)}
                  </div>
                  <div className="mt-2 flex justify-end">
                    <Button
                      size="sm"
                      tone="ghost"
                      onClick={() => void acceptCandidate(candidate.id)}
                    >
                      确认
                    </Button>
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
        <div className="border-t border-line px-3 py-2 text-[9.5px] leading-4 text-muted">
          未确认内容以虚线显示，不进入正文事实。
        </div>
      </aside>
    </div>
  );
}

function MessageBubble({
  message,
  onCapture,
}: {
  message: ChatMessage;
  onCapture: (kind: string) => void;
}) {
  if (message.role === "user") {
    return (
      <div className="flex justify-end">
        <div className="max-w-[80%] rounded-xl rounded-tr-sm bg-teal px-4 py-2.5 text-[12px] leading-6 whitespace-pre-wrap text-paper-raised">
          {message.content}
        </div>
      </div>
    );
  }
  if (message.role === "system") return null;
  return (
    <div className="group flex gap-3">
      <div className="mt-1 flex h-6 w-6 shrink-0 items-center justify-center rounded-full bg-teal text-[9px] font-semibold text-paper-raised">
        AI
      </div>
      <div className="max-w-[86%]">
        <div className="rounded-xl rounded-tl-sm border border-line bg-paper-raised px-4 py-3 text-[12px] leading-6 whitespace-pre-wrap text-ink-soft">
          {message.content}
        </div>
        <div className="mt-1.5 flex items-center gap-2 opacity-0 transition group-hover:opacity-100">
          <DropdownMenu.Root>
            <DropdownMenu.Trigger asChild>
              <button className="rounded-md px-1.5 py-0.5 text-[9.5px] text-muted hover:bg-ink/5 hover:text-teal">
                采纳为…
              </button>
            </DropdownMenu.Trigger>
            <DropdownMenu.Portal>
              <DropdownMenu.Content
                align="start"
                sideOffset={4}
                className="z-50 min-w-[150px] rounded-xl border border-line bg-paper-raised p-1 shadow-xl"
              >
                {captureKinds.map((kind) => (
                  <DropdownMenu.Item
                    key={kind.key}
                    onSelect={() => onCapture(kind.key)}
                    className="cursor-pointer rounded-lg px-2.5 py-1.5 text-[11px] text-ink-soft outline-none hover:bg-teal-soft/70 hover:text-teal"
                  >
                    {kind.label}
                  </DropdownMenu.Item>
                ))}
              </DropdownMenu.Content>
            </DropdownMenu.Portal>
          </DropdownMenu.Root>
          {message.pinned ? <Badge tone="amber">会话约束</Badge> : null}
          <span className="text-[9px] text-muted/70">{message.model || "AI"}</span>
        </div>
      </div>
    </div>
  );
}

function IconAction({
  label,
  onClick,
  children,
}: {
  label: string;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <button
      title={label}
      onClick={onClick}
      className="focus-ring flex h-6 w-7 items-center justify-center rounded-md text-muted transition hover:bg-ink/5 hover:text-ink"
    >
      {children}
    </button>
  );
}

function scopeLabel(scope: string) {
  return { seed: "孵化", plot: "剧情", scene: "场景", lookup: "设定", audit: "审计" }[scope] ?? scope;
}

function modeLabel(mode: string) {
  return { diverge: "发散", structure: "结构", plot: "推进", direct: "续写", lookup: "查设定" }[mode] ?? mode;
}

function candidateLabel(kind: string) {
  return (
    {
      entity: "人物 / 设定",
      claim: "事实",
      event: "事件",
      relation: "关系",
      promise: "伏笔",
      rule: "规则",
      style_sample: "文笔样本",
      scene_goal: "章节目标",
    }[kind] ?? kind
  );
}

export function summaryOf(candidate: Candidate): string {
  const p = candidate.payload ?? {};
  if (candidate.kind === "entity") {
    return `${p.canonicalName ?? p.name ?? "未命名"}${p.description ? `：${String(p.description).slice(0, 60)}` : ""}`;
  }
  if (candidate.kind === "claim") {
    return `${p.subject ?? ""} · ${p.predicate ?? ""} · ${p.object ?? ""}`;
  }
  if (candidate.kind === "event") {
    return String(p.title ?? p.description ?? "事件");
  }
  if (candidate.kind === "relation") {
    return `${p.from ?? ""} → ${p.to ?? ""}（${p.type ?? ""}）`;
  }
  if (candidate.kind === "promise") {
    return `${p.title ?? "伏笔"}：${String(p.contract ?? p.payoffContract ?? "").slice(0, 60)}`;
  }
  return String(p.text ?? p.description ?? p.title ?? "候选内容").slice(0, 90);
}
