import { Fragment, useCallback, useEffect, useRef, useState } from "react";
import {
  BookOpen,
  Check,
  LoaderCircle,
  Layers3,
  Pencil,
  Play,
  Plus,
  Square,
  Wand2,
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
import {
  ipc,
  onGenerationDelta,
  onGenerationError,
  onGenerationFinished,
  type Candidate,
  type ChapterRecord,
  type ContextPreview,
  type GenerationRunRow,
  type MotifHint,
  type RevisionRecord,
  type SceneRecord,
  type StyleCheck,
  type StyleProfile,
} from "@/lib/ipc";
import { useAppStore } from "@/stores/ui";

type SceneForm = {
  id: string | null;
  goal: string;
  conflict: string;
  pov: string;
  storyTime: string;
  placeId: string;
  styleProfileId: string;
  target: string;
  intensity: number;
  entry: string;
  exit: string;
  turn: string;
  channels: string;
  motifs: string;
  distance: string;
};

const emptyScene: SceneForm = {
  id: null,
  goal: "",
  conflict: "",
  pov: "",
  storyTime: "",
  placeId: "",
  styleProfileId: "",
  target: "",
  intensity: 3,
  entry: "",
  exit: "",
  turn: "",
  channels: "视觉，触觉",
  motifs: "",
  distance: "中景",
};

export function StudioPage() {
  const [chapters, setChapters] = useState<ChapterRecord[]>([]);
  const [scenes, setScenes] = useState<SceneRecord[]>([]);
  const [styles, setStyles] = useState<StyleProfile[]>([]);
  const [chapterId, setChapterId] = useState<string | null>(null);
  const [sceneId, setSceneId] = useState<string | null>(null);
  const [chapterText, setChapterText] = useState("");
  const [editorText, setEditorText] = useState("");
  const [output, setOutput] = useState("");
  const [streaming, setStreaming] = useState("");
  const [runId, setRunId] = useState<string | null>(null);
  const [lastCandidate, setLastCandidate] = useState<string | null>(null);
  const [drafts, setDrafts] = useState<Candidate[]>([]);
  const [runs, setRuns] = useState<GenerationRunRow[]>([]);
  const [revisions, setRevisions] = useState<RevisionRecord[]>([]);
  const [historyOpen, setHistoryOpen] = useState(false);
  const [sceneForm, setSceneForm] = useState<SceneForm | null>(null);
  const [context, setContext] = useState<ContextPreview | null>(null);
  const [styleReport, setStyleReport] = useState<StyleCheck | null>(null);
  const [motifHints, setMotifHints] = useState<MotifHint[]>([]);
  const [instruction, setInstruction] = useState("");
  const [styleOverride, setStyleOverride] = useState("");
  const [generationMode, setGenerationMode] = useState<"quick" | "refined">("quick");
  const [centerTab, setCenterTab] = useState<"generate" | "chapter">("generate");
  const [showScenes, setShowScenes] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const streamRef = useRef("");
  const refreshProject = useAppStore((s) => s.refreshProject);

  const currentChapter = chapters.find((chapter) => chapter.id === chapterId) ?? null;
  const currentScene = scenes.find((scene) => scene.id === sceneId) ?? null;

  const loadDrafts = useCallback(async () => {
    const list = await ipc.candidateList("pending", "draft");
    setDrafts(list);
  }, []);

  const loadChapter = useCallback(async (id: string) => {
    const [sceneList, text, revisionList] = await Promise.all([
      ipc.scenes(id),
      ipc.chapterText(id),
      ipc.revisions(id),
    ]);
    setScenes(sceneList);
    setSceneId((current) =>
      sceneList.some((scene) => scene.id === current) ? current : (sceneList[0]?.id ?? null),
    );
    setChapterText(text);
    setEditorText(text);
    setRevisions(revisionList);
    setOutput("");
    setStyleReport(null);
    setMotifHints([]);
  }, []);

  useEffect(() => {
    (async () => {
      try {
        const [chapterList, styleList, draftList, runList] = await Promise.all([
          ipc.chapters(),
          ipc.styleProfiles(),
          ipc.candidateList("pending", "draft"),
          ipc.generationRuns(),
        ]);
        setChapters(chapterList);
        setStyles(styleList);
        setDrafts(draftList);
        setRuns(runList);
        if (chapterList[0]) {
          setChapterId(chapterList[0].id);
          await loadChapter(chapterList[0].id);
        }
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
      } finally {
        setLoading(false);
      }
    })();
  }, [loadChapter]);

  useEffect(() => {
    let disposeDelta = () => {};
    let disposeFinished = () => {};
    let disposeError = () => {};
    void onGenerationDelta((event) => {
      streamRef.current += event.delta;
      setStreaming(streamRef.current);
    }).then((fn) => (disposeDelta = fn));
    void onGenerationFinished((event) => {
      setRunId(null);
      setStreaming("");
      setOutput(streamRef.current);
      setLastCandidate(event.candidateId);
      streamRef.current = "";
      void loadDrafts();
      void ipc.generationRuns().then(setRuns);
    }).then((fn) => (disposeFinished = fn));
    void onGenerationError((event) => {
      setError(event.error);
      setRunId(null);
      setOutput((prev) => prev || streamRef.current);
      setStreaming("");
      streamRef.current = "";
    }).then((fn) => (disposeError = fn));
    return () => {
      disposeDelta();
      disposeFinished();
      disposeError();
    };
  }, [loadDrafts]);

  const selectChapter = async (id: string) => {
    setChapterId(id);
    setBusy(true);
    try {
      await loadChapter(id);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const generate = async () => {
    if (!currentScene) {
      setError("请先建立本章写作任务");
      return;
    }
    setBusy(true);
    setError(null);
    setOutput("");
    setStreaming("");
    streamRef.current = "";
    setStyleReport(null);
    setMotifHints([]);
    try {
      const result = await ipc.generationStart({
        sceneId: currentScene.id,
        mode: generationMode,
        styleProfileId: styleOverride || currentScene.styleProfileId,
        extraInstruction: instruction.trim() || null,
      });
      setRunId(result.runId);
      setCenterTab("generate");
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const cancel = async () => {
    if (runId) await ipc.generationCancel(runId);
  };

  const previewContext = async () => {
    if (!currentScene) return;
    setBusy(true);
    try {
      setContext(
        await ipc.contextPreview(
          currentScene.id,
          12000,
          styleOverride || currentScene.styleProfileId,
        ),
      );
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const checkStyle = async () => {
    const profileId = styleOverride || currentScene?.styleProfileId || styles[0]?.id;
    if (!output || !currentScene) {
      setError("需要先有正文和本章写作任务");
      return;
    }
    setBusy(true);
    try {
      const [style, motifs] = await Promise.all([
        profileId ? ipc.styleCheck(output, profileId) : Promise.resolve(null),
        ipc.motifDensityCheck(currentScene.id, output),
      ]);
      setStyleReport(style);
      setMotifHints(motifs.hints);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const acceptDraft = async (
    candidateId: string,
    useRefined = true,
    acknowledge = false,
  ) => {
    if (!chapterId) return;
    setBusy(true);
    try {
      if (acknowledge) {
        const approved = window.confirm(
          "精修版本改变了可核实事实。确认要连同这些变化一起写入正文吗？",
        );
        if (!approved) return;
      }
      await ipc.draftAcceptVariant(candidateId, chapterId, useRefined, acknowledge);
      await loadChapter(chapterId);
      await loadDrafts();
      await refreshProject();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const saveChapter = async () => {
    if (!chapterId) return;
    setBusy(true);
    try {
      await ipc.chapterTextSave(chapterId, editorText);
      setChapterText(editorText);
      setRevisions(await ipc.revisions(chapterId));
      await refreshProject();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const rollback = async (revisionId: string) => {
    if (!chapterId) return;
    if (!window.confirm("回滚会新建一个版本，不会删除历史。继续吗？")) return;
    setBusy(true);
    try {
      await ipc.revisionRollback(revisionId);
      await loadChapter(chapterId);
      setHistoryOpen(false);
      await refreshProject();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const openNewScene = () =>
    setSceneForm({
      ...emptyScene,
      goal: currentChapter?.title ?? "",
      styleProfileId: styles.find((style) => style.scope === "project")?.id ?? "",
    });

  const openEditScene = (scene: SceneRecord) =>
    setSceneForm({
      id: scene.id,
      goal: scene.goal,
      conflict: scene.conflict,
      pov: scene.pov,
      storyTime: scene.storyTime,
      placeId: scene.placeId ?? "",
      styleProfileId: scene.styleProfileId ?? "",
      target: scene.emotionBeat.target,
      intensity: scene.emotionBeat.intensity,
      entry: scene.emotionBeat.entry,
      exit: scene.emotionBeat.exit,
      turn: scene.emotionBeat.turn,
      channels: scene.sensoryPlan.channels.join("，"),
      motifs: scene.sensoryPlan.motifs.join("，"),
      distance: scene.sensoryPlan.distance,
    });

  const saveScene = async () => {
    if (!sceneForm || !chapterId) return;
    setBusy(true);
    try {
      const scene = await ipc.sceneSave({
        id: sceneForm.id,
        chapterId,
        goal: sceneForm.goal,
        conflict: sceneForm.conflict,
        pov: sceneForm.pov,
        storyTime: sceneForm.storyTime,
        placeId: sceneForm.placeId || null,
        styleProfileId: sceneForm.styleProfileId || null,
        emotionBeat: {
          target: sceneForm.target,
          intensity: sceneForm.intensity,
          entry: sceneForm.entry,
          exit: sceneForm.exit,
          turn: sceneForm.turn,
        },
        sensoryPlan: {
          channels: split(sceneForm.channels),
          motifs: split(sceneForm.motifs),
          distance: sceneForm.distance,
        },
      });
      setSceneForm(null);
      await loadChapter(chapterId);
      setSceneId(scene.id);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  if (loading) {
    return (
      <div className="flex h-64 items-center justify-center text-[12px] text-muted">
        <LoaderCircle size={15} className="mr-2 animate-spin" /> 正在打开正文工坊
      </div>
    );
  }

  if (!chapters.length) {
    return (
      <div className="fade-in mx-auto max-w-[1000px] px-8 py-7">
        <SectionTitle eyebrow="Studio" title="正文工坊" />
        <EmptyState
          icon={<BookOpen size={20} />}
          title="还没有可写的章节"
          description="先在卷章规划中建立并确认章节。正文默认按章写作，只有需要多地点、多视角或多时间点时再拆分场次。"
        />
      </div>
    );
  }

  return (
    <div className="fade-in mx-auto max-w-[1280px] px-6 py-6">
      <SectionTitle
        eyebrow="Studio"
        title="正文工坊"
        action={
          <div className="flex items-center gap-2">
            <Badge tone="teal">{generationMode === "refined" ? "精修模式" : "快速模式"}</Badge>
            {currentChapter?.currentRevisionId ? <Badge tone="neutral">已有版本</Badge> : null}
            <Button
              size="sm"
              tone="ghost"
              onClick={() => setShowScenes((value) => !value)}
              icon={<Layers3 size={12} />}
            >
              {showScenes ? "收起分场" : "分场编辑"}
            </Button>
          </div>
        }
      />
      {error ? (
        <div className="mb-4 rounded-lg border border-coral/25 bg-coral-soft/60 px-4 py-2.5 text-[11.5px] leading-5 text-coral">
          {error}
        </div>
      ) : null}
      <div className="grid grid-cols-[260px_minmax(0,1fr)] gap-4">
        <aside className="paper-card flex h-[680px] min-h-0 flex-col overflow-hidden">
          <div className="border-b border-line px-3.5 py-2.5 text-[11px] font-semibold text-ink-soft">
            章节
          </div>
          <div className="min-h-0 flex-1 overflow-y-auto p-1.5">
            {chapters.map((chapter, index) => {
              const previous = chapters[index - 1];
              const newVolume =
                Boolean(chapter.volumeTitle) &&
                chapter.volumeId !== previous?.volumeId;
              return (
                <Fragment key={chapter.id}>
                  {newVolume ? (
                    <div className="mb-1 mt-3 px-2.5 text-[9px] font-semibold tracking-[0.14em] text-muted first:mt-0">
                      {chapter.volumeTitle}
                    </div>
                  ) : null}
                  <div className="mb-1">
                    <button
                      onClick={() => void selectChapter(chapter.id)}
                      className={`flex w-full items-center gap-2 rounded-lg px-2.5 py-2 text-left transition ${
                        chapterId === chapter.id ? "bg-teal-soft/70" : "hover:bg-ink/[0.03]"
                      }`}
                    >
                      <span className="w-5 text-[9.5px] text-muted">{chapter.number}</span>
                      <span className="flex-1 truncate text-[11.5px] font-medium text-ink">
                        {chapter.title}
                      </span>
                      {chapter.sceneCount > 1 ? (
                        <span className="text-[9px] text-muted">{chapter.sceneCount} 场</span>
                      ) : null}
                    </button>
                    {showScenes && chapterId === chapter.id ? (
                      <div className="ml-4 mt-1 space-y-0.5 border-l border-line pl-2">
                        {scenes.map((scene) => (
                          <button
                            key={scene.id}
                            onClick={() => {
                              setSceneId(scene.id);
                              setCenterTab("generate");
                            }}
                            className={`flex w-full items-center gap-1.5 rounded-md px-2 py-1.5 text-left text-[10.5px] transition ${
                              sceneId === scene.id
                                ? "bg-paper text-teal"
                                : "text-muted hover:bg-ink/[0.03] hover:text-ink"
                            }`}
                          >
                            <span className="h-1 w-1 rounded-full bg-current opacity-50" />
                            <span className="truncate">
                              分场 {scene.number} · {scene.goal || "未命名"}
                            </span>
                          </button>
                        ))}
                        <button
                          onClick={openNewScene}
                          className="flex w-full items-center gap-1.5 rounded-md px-2 py-1.5 text-[10px] text-teal/70 hover:bg-teal-soft/40"
                        >
                          <Plus size={10} /> 添加分场
                        </button>
                      </div>
                    ) : null}
                  </div>
                </Fragment>
              );
            })}
          </div>
        </aside>

        <div className="grid grid-cols-[minmax(0,1fr)_280px] gap-4">
          <section className="paper-card flex h-[680px] min-h-0 flex-col overflow-hidden">
            <div className="flex items-center justify-between border-b border-line px-5 py-3">
              <div className="flex items-center gap-4">
                <div className="flex rounded-full border border-line p-0.5">
                  {[
                    { key: "generate" as const, label: "生成" },
                    { key: "chapter" as const, label: "正文" },
                  ].map((tab) => (
                    <button
                      key={tab.key}
                      onClick={() => setCenterTab(tab.key)}
                      className={`rounded-full px-3 py-1 text-[10px] font-medium transition ${
                        centerTab === tab.key
                          ? "bg-teal text-paper-raised"
                          : "text-muted hover:text-ink"
                      }`}
                    >
                      {tab.label}
                    </button>
                  ))}
                </div>
                {currentScene ? (
                  <span className="text-[10px] text-muted">
                    {showScenes ? `分场 ${currentScene.number}` : "本章任务"} ·{" "}
                    {currentScene.status}
                  </span>
                ) : null}
              </div>
              {currentScene ? (
                <Button size="sm" tone="ghost" onClick={() => openEditScene(currentScene)} icon={<Pencil size={11} />}>
                  写作要点
                </Button>
              ) : null}
            </div>

            {centerTab === "generate" ? (
              <>
                <div className="border-b border-line px-5 py-3">
                  {currentScene ? (
                    <>
                      <div className="flex items-start justify-between gap-4">
                        <div>
                          <div className="display text-[17px] text-ink">{currentScene.goal}</div>
                          <div className="mt-1 text-[11px] leading-5 text-muted">
                            冲突：{currentScene.conflict}
                          </div>
                        </div>
                        <div className="flex shrink-0 gap-1.5">
                          <Badge tone="amber">
                            {currentScene.emotionBeat.target || "情绪未设定"} · {currentScene.emotionBeat.intensity}/5
                          </Badge>
                          <Badge tone="neutral">{currentScene.pov || "POV 未设定"}</Badge>
                        </div>
                      </div>
                      <div className="mt-2 flex items-center gap-3 text-[9.5px] text-muted">
                        <span>感官：{currentScene.sensoryPlan.channels.join(" / ") || "未设定"}</span>
                        <span>母题：{currentScene.sensoryPlan.motifs.join(" / ") || "未设定"}</span>
                        <span>距离：{currentScene.sensoryPlan.distance || "未设定"}</span>
                      </div>
                    </>
                  ) : (
                    <div className="flex items-center justify-between">
                      <span className="text-[11.5px] text-muted">
                        本章还未建立写作任务。人物、冲突和 POV 可以在同一条任务中完成。
                      </span>
                      <Button size="sm" onClick={openNewScene} icon={<Plus size={11} />}>
                        建立本章任务
                      </Button>
                    </div>
                  )}
                </div>

                <div className="min-h-0 flex-1 overflow-y-auto px-6 py-5">
                  {streaming || output ? (
                    <div className="mx-auto max-w-[720px]">
                      <div className="mb-3 flex items-center justify-between">
                        <span className="text-[10px] font-medium tracking-wide text-muted">
                          {streaming ? "正在生成…" : "生成结果（尚未写入正文）"}
                        </span>
                        <span className="text-[10px] text-muted">
                          {(streaming || output).length} 字
                        </span>
                      </div>
                      <div className="whitespace-pre-wrap font-display text-[14px] leading-8 text-ink">
                        {streaming || output}
                        {streaming ? (
                          <span className="ml-1 inline-block h-4 w-0.5 animate-pulse bg-teal align-middle" />
                        ) : null}
                      </div>
                      {!streaming && output ? (
                        <div className="mt-5 flex items-center gap-2">
                          <Button size="sm" onClick={() => void checkStyle()}>
                            文笔检查
                          </Button>
                          {lastCandidate ? (
                            <Button
                              size="sm"
                              tone="primary"
                              onClick={() => void acceptDraft(lastCandidate)}
                              disabled={busy}
                              icon={<Check size={12} />}
                            >
                              接受为正文
                            </Button>
                          ) : null}
                        </div>
                      ) : null}
                    </div>
                  ) : (
                    <div className="flex h-full flex-col items-center justify-center text-center">
                      <div className="mb-4 flex h-11 w-11 items-center justify-center rounded-full border border-teal/20 bg-teal-soft/40 text-teal">
                        <Wand2 size={18} />
                      </div>
                      <div className="display text-[18px] text-ink">建立任务后直接起草本章</div>
                      <p className="mt-2 max-w-sm text-[11px] leading-6 text-muted">
                        场景已降为可选的分场工具。单线章节只需填写目标、冲突与 POV；系统会编译九层上下文并注入人物声音。
                      </p>
                    </div>
                  )}
                </div>

                <div className="border-t border-line px-4 py-3">
                  <div className="flex items-end gap-2">
                    <div className="min-w-0 flex-1">
                      <Input
                        value={instruction}
                        onChange={(e) => setInstruction(e.target.value)}
                        placeholder="额外要求，例如：把雨声写成压迫感，不要解释父亲为什么离开"
                        disabled={Boolean(runId)}
                      />
                    </div>
                    <select
                      value={styleOverride}
                      onChange={(e) => setStyleOverride(e.target.value)}
                      className="h-10 max-w-[150px] rounded-[10px] border border-line-strong bg-paper-raised px-2 text-[10.5px]"
                    >
                      <option value="">跟随本章文笔</option>
                      {styles.map((style) => (
                        <option key={style.id} value={style.id}>
                          {style.name}
                        </option>
                      ))}
                    </select>
                    <select
                      value={generationMode}
                      onChange={(e) => setGenerationMode(e.target.value as "quick" | "refined")}
                      className="h-10 rounded-[10px] border border-line-strong bg-paper-raised px-2 text-[10.5px]"
                      disabled={Boolean(runId)}
                    >
                      <option value="quick">快速</option>
                      <option value="refined">精修</option>
                    </select>
                    {runId ? (
                      <Button size="md" onClick={() => void cancel()} icon={<Square size={12} />}>
                        停止
                      </Button>
                    ) : (
                      <Button
                        size="md"
                        tone="primary"
                        onClick={() => void generate()}
                        disabled={!currentScene || busy}
                        icon={<Play size={13} />}
                      >
                        生成草稿
                      </Button>
                    )}
                  </div>
                </div>
              </>
            ) : (
              <>
                <div className="min-h-0 flex-1 overflow-y-auto px-6 py-5">
                  <div className="mx-auto max-w-[720px]">
                    <div className="mb-3 flex items-center justify-between">
                      <span className="text-[10px] font-medium tracking-wide text-muted">
                        当前正文 · {chapterText.length} 字
                      </span>
                      <div className="flex gap-1.5">
                        <Button size="sm" onClick={() => setHistoryOpen(true)}>
                          版本历史
                        </Button>
                        <Button
                          size="sm"
                          tone="primary"
                          onClick={() => void saveChapter()}
                          disabled={busy || editorText === chapterText}
                        >
                          保存新版本
                        </Button>
                      </div>
                    </div>
                    <Textarea
                      value={editorText}
                      onChange={(e) => setEditorText(e.target.value)}
                      className="min-h-[480px] border-transparent bg-transparent px-0 font-display text-[14px] leading-8 focus:border-transparent"
                      placeholder="在这里写作，或从生成结果接受草稿…"
                    />
                  </div>
                </div>
              </>
            )}
          </section>

          <aside className="flex h-[680px] min-h-0 flex-col gap-3">
            <section className="paper-card p-3.5">
              <div className="mb-2 flex items-center justify-between">
                <span className="text-[10.5px] font-semibold text-ink-soft">上下文预检</span>
                <Button size="sm" tone="ghost" onClick={() => void previewContext()} disabled={!currentScene}>
                  编译
                </Button>
              </div>
              {context ? (
                <div className="space-y-1">
                  <div className="text-[9.5px] leading-4 text-muted">
                    {context.totalChars} / {context.budget} 字 · {context.contextHash.slice(0, 10)}
                  </div>
                  {context.sections.map((section) => (
                    <div
                      key={section.layer}
                      className="flex items-center justify-between text-[9.5px]"
                    >
                      <span className="text-muted">
                        {section.layer} {section.title}
                      </span>
                      <span className="text-ink-soft">
                        {section.chars}/{section.budget}
                      </span>
                    </div>
                  ))}
                </div>
              ) : (
                <div className="text-[9.5px] leading-4 text-muted">
                  生成前可查看九层上下文与哈希。缺少证据时会明确标注。
                </div>
              )}
            </section>

            <section className="paper-card flex min-h-0 flex-1 flex-col overflow-hidden">
              <div className="border-b border-line px-3.5 py-2.5 text-[10.5px] font-semibold text-ink-soft">
                草稿候选 {drafts.length}
              </div>
              <div className="min-h-0 flex-1 overflow-y-auto p-2">
                {drafts.length === 0 ? (
                  <div className="px-2 py-6 text-center text-[9.5px] leading-5 text-muted">
                    生成或“继续写”的草稿会进入这里，接受后才写入正文。
                  </div>
                ) : (
                  drafts.map((draft) => {
                    const factDelta = draft.payload.factDelta;
                    const blocked = Boolean(factDelta?.blocked);
                    const refined = draft.kind === "draft_refined";
                    return (
                    <div key={draft.id} className="mb-2 rounded-lg border border-line bg-paper/60 p-2.5">
                      <div className="text-[9px] text-muted">
                        {draft.payload.source === "chat_continue"
                          ? "对话续写"
                          : refined
                            ? "精修草稿"
                            : "章节生成"}{" "}
                        ·{" "}
                        {draft.payload.wordCount ?? "—"} 字
                      </div>
                      <p className="mt-1 line-clamp-4 text-[10px] leading-4 text-ink-soft">
                        {String(draft.payload.text ?? "").slice(0, 160)}
                      </p>
                      {factDelta?.changes?.length ? (
                        <div className="mt-1.5 rounded bg-coral-soft/60 px-1.5 py-1 text-[8.5px] leading-4 text-coral">
                          {factDelta.changes
                            .slice(0, 2)
                            .map((change: { message: string }) => change.message)
                            .join("；")}
                        </div>
                      ) : null}
                      {factDelta?.possibleInflation ? (
                        <div className="mt-1 text-[8.5px] text-amber">
                          字数增长超过 25%，可能注水
                        </div>
                      ) : null}
                      <div className="mt-1.5 flex justify-end gap-1">
                        {refined ? (
                          <Button
                            size="sm"
                            tone="ghost"
                            onClick={() => void acceptDraft(draft.id, false)}
                            disabled={!chapterId}
                          >
                            用原文
                          </Button>
                        ) : null}
                        <Button
                          size="sm"
                          tone="ghost"
                          onClick={() => void acceptDraft(draft.id, refined, blocked)}
                          disabled={!chapterId}
                        >
                          {refined ? (blocked ? "确认并写入" : "接受精修") : "写入"}
                        </Button>
                      </div>
                    </div>
                  );})
                )}
              </div>
            </section>

            <section className="paper-card max-h-[270px] overflow-y-auto p-3.5">
              <div className="mb-2 flex items-center justify-between text-[10.5px] font-semibold text-ink-soft">
                <span>文笔漂移</span>
                <span className="text-[9px] text-muted">{styleReport ? `${styleReport.drifts.length} 项` : "未检查"}</span>
              </div>
              {styleReport ? (
                styleReport.drifts.length ? (
                  <div className="space-y-1.5">
                    {styleReport.drifts.slice(0, 4).map((drift) => (
                      <div key={drift.dimension} className="text-[9px] leading-4">
                        <span className={drift.severity === "major" ? "text-coral" : "text-amber"}>
                          {drift.dimension}
                        </span>
                        <span className="text-muted">
                          {" "}
                          {drift.current} · 目标 {drift.target}
                        </span>
                      </div>
                    ))}
                  </div>
                ) : (
                  <div className="text-[9.5px] leading-4 text-teal">暂未检测到明显漂移。</div>
                )
              ) : (
                <div className="text-[9.5px] leading-4 text-muted">生成后运行检查，只提示不自动改写。</div>
              )}
              {motifHints.length ? (
                <div className="mt-3 border-t border-line pt-2">
                  <div className="mb-1 text-[9.5px] font-medium text-amber">母题密度</div>
                  {motifHints.slice(0, 3).map((hint) => (
                    <div key={`${hint.motif}-${hint.count}`} className="mb-1 text-[8.5px] leading-4 text-muted">
                      <span className={hint.severity === "major" ? "text-coral" : "text-amber"}>
                        {hint.motif}
                      </span>
                      <span> {hint.message}</span>
                    </div>
                  ))}
                </div>
              ) : null}
            </section>

            <section className="paper-card p-3.5">
              <div className="mb-1.5 text-[10px] font-semibold text-ink-soft">最近运行</div>
              {runs.slice(0, 3).map((run) => (
                <div key={run.id} className="flex items-center justify-between py-0.5 text-[9px] text-muted">
                  <span className="truncate">{run.model}</span>
                  <span className={run.status === "done" ? "text-teal" : "text-amber"}>
                    {run.status}
                  </span>
                </div>
              ))}
              {!runs.length ? <div className="text-[9px] text-muted">还没有生成运行。</div> : null}
            </section>
          </aside>
        </div>
      </div>

      <SceneEditor
        form={sceneForm}
        styles={styles}
        onChange={setSceneForm}
        onClose={() => setSceneForm(null)}
        onSave={saveScene}
      />
      <ContextModal context={context} onClose={() => setContext(null)} />
      <Modal
        open={historyOpen}
        onOpenChange={setHistoryOpen}
        title="版本历史"
        description="每次保存、接受草稿和批量审核都会留下 revision。回滚同样会生成新版本。"
        width="max-w-[620px]"
        footer={
          <Button tone="ghost" onClick={() => setHistoryOpen(false)}>
            关闭
          </Button>
        }
      >
        <div className="space-y-2">
          {revisions.map((revision) => (
            <div
              key={revision.id}
              className="flex items-center justify-between rounded-lg border border-line bg-paper/60 px-3 py-2.5"
            >
              <div>
                <div className="text-[11px] font-medium text-ink">
                  {revision.authorAction}
                </div>
                <div className="mt-0.5 font-mono text-[9px] text-muted">
                  {revision.id.slice(0, 8)} · {new Date(revision.createdAt).toLocaleString("zh-CN")}
                </div>
              </div>
              <Button size="sm" tone="ghost" onClick={() => void rollback(revision.id)}>
                回滚到此
              </Button>
            </div>
          ))}
          {!revisions.length ? (
            <div className="py-8 text-center text-[11px] text-muted">还没有版本记录</div>
          ) : null}
        </div>
      </Modal>
    </div>
  );
}

function SceneEditor({
  form,
  styles,
  onChange,
  onClose,
  onSave,
}: {
  form: SceneForm | null;
  styles: StyleProfile[];
  onChange: (form: SceneForm) => void;
  onClose: () => void;
  onSave: () => Promise<void>;
}) {
  const set = <K extends keyof SceneForm>(key: K, value: SceneForm[K]) => {
    if (form) onChange({ ...form, [key]: value });
  };
  return (
    <Modal
      open={Boolean(form)}
      onOpenChange={(open) => !open && onClose()}
      title={form?.id ? "编辑写作任务" : "建立本章写作任务"}
      description="单线章节按章填写即可。需要多地点、多视角或多时间点时，再为本章添加分场。"
      width="max-w-[720px]"
      footer={
        <>
          <Button tone="ghost" onClick={onClose}>
            取消
          </Button>
          <Button
            tone="primary"
            onClick={() => void onSave()}
            disabled={!form?.goal.trim() || !form?.conflict.trim()}
          >
            保存写作任务
          </Button>
        </>
      }
    >
      {form ? (
        <div className="space-y-3">
          <Field label="本章目标" hint="这一段要改变什么">
            <Textarea rows={2} value={form.goal} onChange={(e) => set("goal", e.target.value)} />
          </Field>
          <Field label="冲突" hint="谁阻止，代价是什么">
            <Textarea
              rows={2}
              value={form.conflict}
              onChange={(e) => set("conflict", e.target.value)}
            />
          </Field>
          <div className="grid grid-cols-3 gap-3">
            <Field label="POV">
              <Input value={form.pov} onChange={(e) => set("pov", e.target.value)} />
            </Field>
            <Field label="故事时间">
              <Input value={form.storyTime} onChange={(e) => set("storyTime", e.target.value)} />
            </Field>
            <Field label="地点">
              <Input value={form.placeId} onChange={(e) => set("placeId", e.target.value)} />
            </Field>
          </div>
          <Field label="文笔档案">
            <select
              value={form.styleProfileId}
              onChange={(e) => set("styleProfileId", e.target.value)}
              className="h-10 w-full rounded-[10px] border border-line-strong bg-paper-raised px-3 text-[12px]"
            >
              <option value="">跟随项目默认</option>
              {styles.map((style) => (
                <option key={style.id} value={style.id}>
                  {style.name} · v{style.version}
                </option>
              ))}
            </select>
          </Field>
          <div className="hairline" />
          <div className="grid grid-cols-3 gap-3">
            <Field label="目标情绪">
              <Input value={form.target} onChange={(e) => set("target", e.target.value)} />
            </Field>
            <Field label="强度 1–5">
              <Input
                type="number"
                min={1}
                max={5}
                value={form.intensity}
                onChange={(e) => set("intensity", Number(e.target.value))}
              />
            </Field>
            <Field label="镜头距离">
              <Input value={form.distance} onChange={(e) => set("distance", e.target.value)} />
            </Field>
          </div>
          <div className="grid grid-cols-2 gap-3">
            <Field label="入场状态">
              <Input value={form.entry} onChange={(e) => set("entry", e.target.value)} />
            </Field>
            <Field label="出场状态">
              <Input value={form.exit} onChange={(e) => set("exit", e.target.value)} />
            </Field>
          </div>
          <Field label="转折">
            <Input value={form.turn} onChange={(e) => set("turn", e.target.value)} />
          </Field>
          <div className="grid grid-cols-2 gap-3">
            <Field label="感官通道" hint="逗号分隔，2–3 个">
              <Input value={form.channels} onChange={(e) => set("channels", e.target.value)} />
            </Field>
            <Field label="母题" hint="1–2 个">
              <Input value={form.motifs} onChange={(e) => set("motifs", e.target.value)} />
            </Field>
          </div>
        </div>
      ) : null}
    </Modal>
  );
}

function ContextModal({
  context,
  onClose,
}: {
  context: ContextPreview | null;
  onClose: () => void;
}) {
  return (
    <Modal
      open={Boolean(context)}
      onOpenChange={(open) => !open && onClose()}
      title="上下文包"
      description="九层上下文，按预算截断并记录哈希。原文证据优先于摘要。"
      width="max-w-[900px]"
      footer={
        <Button tone="ghost" onClick={onClose}>
          关闭
        </Button>
      }
    >
      {context ? (
        <div className="space-y-2.5">
          <div className="flex items-center justify-between rounded-lg border border-line bg-paper px-3 py-2 text-[10px]">
            <span className="text-muted">
              {context.totalChars} / {context.budget} 字
            </span>
            <span className="font-mono text-ink-soft">{context.contextHash.slice(0, 20)}</span>
          </div>
          {context.sections.map((section) => (
            <div key={section.layer} className="rounded-lg border border-line">
              <div className="flex items-center justify-between border-b border-line bg-paper/60 px-3 py-1.5">
                <span className="text-[10px] font-semibold text-teal">
                  {section.layer} {section.title}
                </span>
                <span className="text-[9px] text-muted">
                  {section.chars}/{section.budget}
                </span>
              </div>
              <pre className="max-h-[160px] overflow-y-auto px-3 py-2 font-mono text-[9.5px] leading-5 whitespace-pre-wrap text-ink-soft">
                {section.content}
              </pre>
            </div>
          ))}
        </div>
      ) : null}
    </Modal>
  );
}

function split(value: string) {
  return value
    .split(/[，,]/)
    .map((item) => item.trim())
    .filter(Boolean);
}
