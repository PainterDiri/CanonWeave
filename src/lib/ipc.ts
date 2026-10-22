import { listen } from "@tauri-apps/api/event";
import {
  commands,
  type CandidateCaptureInput,
  type ChatSessionCreateInput,
  type CharacterInput,
  type ModelConfig,
  type OutlineGenerateInput,
  type OutlineUpdateInput,
  type PremiseInput,
  type PromiseInput,
  type PromiseStateInput,
  type ProjectSnapshot,
  type SceneInput,
  type BeatOption,
  type StyleProfileInput,
  type StyleSampleInput,
  type ReviewPolicyInput,
  type IssueActionInput,
  type PreferenceInput,
  type WikiEditInput,
  type VoiceSampleInput,
  type VoiceThresholdInput,
  type PromisePayoffInput,
  type ExportInput,
  type KnowledgeDomainInput,
  type KnowledgeEntryInput,
  type NarrativeGenerateInput,
  type NarrativeBlueprintEditInput,
  type AgentRunCreateInput,
} from "./bindings";

export type {
  Candidate,
  ChatMessage,
  ChatSession,
  CharacterDetail,
  ModelConfig,
  OutlineNode,
  OutlineGenerateInput,
  OutlineUpdateInput,
  Premise,
  PremiseInput,
  CharacterInput,
  PromiseInput,
  PromiseStateInput,
  PromisePayoffInput,
  PayoffCheck,
  ChatSessionCreateInput,
  CandidateCaptureInput,
  PromiseRecord,
  ProjectSnapshot,
  ReviewQueue,
  SourceDocument,
  StyleProfile,
  StylePreset,
  StyleSample,
  StyleMetrics,
  StyleCheck,
  StyleCompare,
  ChapterRecord,
  SceneRecord,
  SceneInput,
  ContextPreview,
  GenerationRunRow,
  BeatOption,
  BatchPreview,
  BatchResult,
  ReviewPolicy,
  PolicyApplyResult,
  BatchUndoResult,
  ReviewSampleRow,
  RuleRecord,
  AuditIssue,
  AuditRunResult,
  RevisionRecord,
  RevisionCompare,
  PreferenceInput,
  BranchRecord,
  TimelineItem,
  GraphData,
  CommunityReport,
  CommunitySummaryResult,
  InsightResult,
  ImpactSet,
  WikiPage,
  WikiPageDependency,
  WikiLlmCompileResult,
  WikiRebuildResult,
  WikiEditInput,
  CharacterVoiceSample,
  CharacterVoiceProfile,
  VoiceCheck,
  VoiceSampleInput,
  VoiceThresholdInput,
  VoiceThresholds,
  VoiceDimensions,
  PreferenceSummary,
  PreferenceTarget,
  MotifRecord,
  MotifCheck,
  MotifHint,
  ExportInput,
  ExportResult,
  KnowledgeDomain,
  KnowledgeDomainInput,
  KnowledgeEntry,
  KnowledgeEntryInput,
  KnowledgeField,
  VolumePlanInput,
  NarrativeBlueprint,
  NarrativeBlueprintView,
  NarrativeGenerateInput,
  NarrativeElement,
  NarrativeThread,
  NarrativeTimelineEvent,
  NarrativeLogicNode,
  NarrativeLogicEdge,
  NarrativePresentation,
  NarrativeDiagnostic,
  NarrativeBlueprintEditInput,
  AgentRun,
  AgentRunCreateInput,
  AgentRunDetail,
  AgentStep,
  AgentEvidence,
  AgentToolDefinition,
  AgentNextAction,
  StyleRecommendation,
  FoundationReport,
  FoundationCheck,
  EmbeddingIndexResult,
  EmbeddingStatus,
  HybridSearchHit,
  HybridSearchResult,
} from "./bindings";
export type { CreateProjectInput, ModelConfigInput, ModelTestResult } from "./bindings";

export const isTauri = (): boolean =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

function guarded<T>(fn: () => Promise<T>): Promise<T> {
  if (!isTauri()) {
    return Promise.reject(new Error("此功能需要在 CanonWeave 桌面应用中使用"));
  }
  return fn();
}

export type IpcResult<T> = { status: "ok"; data: T } | { status: "error"; error: string };

export function unwrap<T>(result: IpcResult<T>): T {
  if (result.status === "ok") return result.data;
  throw new Error(result.error);
}

const browserModel: ModelConfig = {
  baseUrl: "https://api.openai.com/v1",
  model: "gpt-4o-mini",
  temperature: 0.8,
  maxTokens: 4096,
  embeddingMode: "off",
  embeddingModel: null,
  embeddingBaseUrl: null,
  embeddingDimensions: null,
  embeddingBatchSize: 32,
  embeddingAutoIndex: false,
  hasApiKey: false,
};

export const ipc = {
  async projectState(): Promise<ProjectSnapshot | null> {
    if (!isTauri()) return null;
    return unwrap(await commands.projectState());
  },
  async createProject(input: {
    name: string;
    rootPath: string;
    mode: string;
    writingLanguage: string | null;
  }): Promise<ProjectSnapshot> {
    if (!isTauri()) throw new Error("创建项目需要在 CanonWeave 桌面应用中完成");
    return unwrap(await commands.projectCreate(input));
  },
  async openProject(rootPath: string): Promise<ProjectSnapshot> {
    if (!isTauri()) throw new Error("打开项目需要在 CanonWeave 桌面应用中完成");
    return unwrap(await commands.projectOpen(rootPath));
  },
  async closeProject(): Promise<void> {
    if (!isTauri()) return;
    unwrap(await commands.projectClose());
  },
  async modelGet(): Promise<ModelConfig> {
    if (!isTauri()) return browserModel;
    return commands.modelGet();
  },
  async modelSave(input: {
    baseUrl: string;
    model: string;
    temperature: number;
    maxTokens: number;
    embeddingMode: string;
    embeddingModel: string | null;
    embeddingBaseUrl: string | null;
    embeddingDimensions: number | null;
    embeddingBatchSize: number;
    embeddingAutoIndex: boolean;
    apiKey: string | null;
  }): Promise<ModelConfig> {
    if (!isTauri()) return { ...browserModel, ...input, hasApiKey: Boolean(input.apiKey) };
    return unwrap(await commands.modelSave(input));
  },
  async modelTest() {
    if (!isTauri()) {
      return {
        ok: false,
        message: "浏览器预览模式无法访问系统钥匙串，请在桌面应用中测试连接",
        latencyMs: 0,
        model: browserModel.model,
      };
    }
    return unwrap(await commands.modelTest());
  },

  seedGenerate: (logline: string, sessionId: string | null = null) =>
    guarded(async () => unwrap(await commands.seedGenerate({ logline, sessionId }))),
  premiseList: () => guarded(async () => unwrap(await commands.premiseList())),
  premiseActive: () => guarded(async () => unwrap(await commands.premiseActive())),
  premiseUpdate: (id: string, input: PremiseInput) =>
    guarded(async () => unwrap(await commands.premiseUpdate(id, input))),
  premiseConfirm: (id: string) =>
    guarded(async () => unwrap(await commands.premiseConfirm(id))),
  premiseDebugSources: (id: string) =>
    guarded(async () => unwrap(await commands.premiseDebugSources(id))),
  blueprintCurrent: () =>
    guarded(async () => unwrap(await commands.blueprintCurrent())),
  blueprintCandidate: () =>
    guarded(async () => unwrap(await commands.blueprintCandidate())),
  blueprintGenerate: (input: NarrativeGenerateInput) =>
    guarded(async () => unwrap(await commands.blueprintGenerate(input))),
  blueprintUpdate: (input: NarrativeBlueprintEditInput) =>
    guarded(async () => unwrap(await commands.blueprintUpdate(input))),
  blueprintConfirm: (blueprintId: string) =>
    guarded(async () => unwrap(await commands.blueprintConfirm(blueprintId))),
  blueprintLogicContext: (limit?: number) =>
    guarded(async () => unwrap(await commands.blueprintLogicContext(limit ?? null))),
  blueprintNodes: () => guarded(async () => unwrap(await commands.blueprintNodes())),

  chatSessions: (scope: string | null, includeArchived = false) =>
    guarded(async () => unwrap(await commands.chatSessionList(scope, includeArchived))),
  chatCreate: (input: ChatSessionCreateInput) =>
    guarded(async () => unwrap(await commands.chatSessionCreate(input))),
  chatUpdate: (
    id: string,
    updates: { title?: string | null; pinned?: boolean | null; mode?: string | null; scope?: string | null },
  ) =>
    guarded(async () =>
      unwrap(
        await commands.chatSessionUpdate(
          id,
          updates.title ?? null,
          updates.pinned ?? null,
          updates.mode ?? null,
          updates.scope ?? null,
        ),
      ),
    ),
  chatArchive: (id: string) => guarded(async () => unwrap(await commands.chatSessionArchive(id))),
  chatDelete: (id: string) => guarded(async () => unwrap(await commands.chatSessionDelete(id))),
  chatFork: (id: string) => guarded(async () => unwrap(await commands.chatSessionFork(id))),
  chatMessages: (sessionId: string) =>
    guarded(async () => unwrap(await commands.chatMessagesList(sessionId))),
  chatSend: (sessionId: string, content: string) =>
    guarded(async () => unwrap(await commands.chatSend({ sessionId, content }))),
  chatCancel: (requestId: string) =>
    guarded(async () => unwrap(await commands.chatCancel(requestId))),
  chatCapture: (input: CandidateCaptureInput) =>
    guarded(async () => unwrap(await commands.chatCapture(input))),
  chatCandidates: (sessionId: string) =>
    guarded(async () => unwrap(await commands.chatCandidates(sessionId))),
  chatAppendUser: (sessionId: string, content: string) =>
    guarded(async () => unwrap(await commands.chatAppendUser(sessionId, content))),
  chatAppendAssistant: (sessionId: string, content: string) =>
    guarded(async () => unwrap(await commands.chatAppendAssistant(sessionId, content))),

  outlineList: () => guarded(async () => unwrap(await commands.outlineList())),
  outlineGenerate: (input: OutlineGenerateInput) =>
    guarded(async () => unwrap(await commands.outlineGenerate(input))),
  outlineUpdate: (input: OutlineUpdateInput) =>
    guarded(async () => unwrap(await commands.outlineUpdate(input))),
  outlineConfirm: (ids: string[]) =>
    guarded(async () => unwrap(await commands.outlineConfirm(ids))),
  outlineDrop: (id: string) => guarded(async () => unwrap(await commands.outlineDrop(id))),
  outlineStatus: () => guarded(async () => unwrap(await commands.outlineStatus())),

  documentImport: (path: string) =>
    guarded(async () => unwrap(await commands.documentImport(path))),
  documentList: () => guarded(async () => unwrap(await commands.documentList())),
  extractionRun: (documentId: string) =>
    guarded(async () => unwrap(await commands.extractionRun(documentId))),

  reviewQueue: () => guarded(async () => unwrap(await commands.reviewQueue())),
  candidateList: (status: string | null, kind: string | null) =>
    guarded(async () => unwrap(await commands.candidateList(status, kind))),
  candidateAccept: (id: string) =>
    guarded(async () => unwrap(await commands.candidateAccept(id))),
  candidateReject: (id: string) =>
    guarded(async () => unwrap(await commands.candidateReject(id))),
  candidateUpdate: (id: string, payloadJson: string) =>
    guarded(async () => unwrap(await commands.candidateUpdatePayload(id, payloadJson))),

  entityList: (entityType: string | null) =>
    guarded(async () => unwrap(await commands.entityList(entityType))),
  characterList: () => guarded(async () => unwrap(await commands.characterList())),
  characterSave: (input: CharacterInput) =>
    guarded(async () => unwrap(await commands.characterSave(input))),

  knowledgeDomains: () => guarded(async () => unwrap(await commands.knowledgeDomains())),
  knowledgeDomainCreate: (input: KnowledgeDomainInput) =>
    guarded(async () => unwrap(await commands.knowledgeDomainCreate(input))),
  knowledgeEntries: (domainId: string | null) =>
    guarded(async () => unwrap(await commands.knowledgeEntries(domainId))),
  knowledgeEntrySave: (input: KnowledgeEntryInput) =>
    guarded(async () => unwrap(await commands.knowledgeEntrySave(input))),

  promiseList: () => guarded(async () => unwrap(await commands.promiseList())),
  promiseSave: (input: PromiseInput) =>
    guarded(async () => unwrap(await commands.promiseSave(input))),
  promiseTransition: (input: PromiseStateInput) =>
    guarded(async () => unwrap(await commands.promiseTransition(input))),
  promisePayoffCheck: (input: PromisePayoffInput) =>
    guarded(async () => unwrap(await commands.promisePayoffCheck(input))),
  promisePayoff: (input: PromisePayoffInput) =>
    guarded(async () => unwrap(await commands.promisePayoff(input))),

  stylePresets: () => guarded(async () => commands.stylePresetList()),
  styleProfiles: () => guarded(async () => unwrap(await commands.styleProfileList())),
  styleProfileGet: (id: string) =>
    guarded(async () => unwrap(await commands.styleProfileGet(id))),
  styleProfileSave: (input: StyleProfileInput) =>
    guarded(async () => unwrap(await commands.styleProfileSave(input))),
  styleApplyPreset: (presetId: string, name: string | null = null) =>
    guarded(async () => unwrap(await commands.styleProfileApplyPreset(presetId, name))),
  styleSamples: (profileId: string | null) =>
    guarded(async () => unwrap(await commands.styleSampleList(profileId))),
  styleSampleAdd: (input: StyleSampleInput) =>
    guarded(async () => unwrap(await commands.styleSampleAdd(input))),
  styleAnalyze: (text: string) =>
    guarded(async () => commands.styleAnalyze(text)),
  styleContract: (profileId: string) =>
    guarded(async () => unwrap(await commands.styleContractText(profileId))),
  styleCheck: (text: string, profileId: string) =>
    guarded(async () => unwrap(await commands.styleCheck(text, profileId))),
  styleCompare: (left: string, right: string) =>
    guarded(async () => commands.styleCompare(left, right)),
  styleCalibrate: (profileId: string) =>
    guarded(async () => unwrap(await commands.styleCalibrate(profileId))),

  chapters: () => guarded(async () => unwrap(await commands.chapterList())),
  scenes: (chapterId: string | null) =>
    guarded(async () => unwrap(await commands.sceneList(chapterId))),
  sceneSave: (input: SceneInput) =>
    guarded(async () => unwrap(await commands.sceneSave(input))),
  sceneContract: (sceneId: string) =>
    guarded(async () => unwrap(await commands.sceneContractText(sceneId))),
  contextPreview: (sceneId: string, budget: number | null = null, styleProfileId: string | null = null) =>
    guarded(async () => unwrap(await commands.contextPreview(sceneId, budget, styleProfileId))),
  chapterText: (chapterId: string) =>
    guarded(async () => unwrap(await commands.chapterTextGet(chapterId))),
  chapterTextSave: (chapterId: string, text: string) =>
    guarded(async () => unwrap(await commands.chapterTextSave(chapterId, text))),
  draftAccept: (candidateId: string, chapterId: string) =>
    guarded(async () => unwrap(await commands.draftAccept(candidateId, chapterId))),
  generationStart: (input: {
    sceneId: string;
    mode: string;
    styleProfileId: string | null;
    extraInstruction: string | null;
  }) => guarded(async () => unwrap(await commands.generationStart(input))),
  generationCancel: (runId: string) =>
    guarded(async () => unwrap(await commands.generationCancel(runId))),
  generationRuns: () => guarded(async () => unwrap(await commands.generationRuns())),
  plotBeats: (sessionId: string, instruction: string) =>
    guarded(async () => unwrap(await commands.plotBeats({ sessionId, instruction }))),
  sceneFromBeat: (chapterId: string, beat: BeatOption) =>
    guarded(async () => unwrap(await commands.sceneFromBeat(chapterId, beat))),
  chatContinue: (sessionId: string) =>
    guarded(async () => unwrap(await commands.chatContinue(sessionId))),
  chatToScene: (sessionId: string, chapterId: string) =>
    guarded(async () => unwrap(await commands.chatToScene(sessionId, chapterId))),
  reviewBatchPreview: (ids: string[] | null = null) =>
    guarded(async () => unwrap(await commands.reviewBatchPreview(ids))),
  reviewBatchApply: (ids: string[], chapterId: string | null = null) =>
    guarded(async () => unwrap(await commands.reviewBatchApply(ids, chapterId))),
  reviewPolicyGet: (scope: string | null = "project", scopeId: string | null = null) =>
    guarded(async () => unwrap(await commands.reviewPolicyGet(scope, scopeId))),
  reviewPolicySave: (input: ReviewPolicyInput) =>
    guarded(async () => unwrap(await commands.reviewPolicySave(input))),
  reviewPolicyApply: (candidateIds: string[] | null = null) =>
    guarded(async () => unwrap(await commands.reviewPolicyApply(candidateIds))),
  reviewBatchUndo: (batchId: string) =>
    guarded(async () => unwrap(await commands.reviewBatchUndo(batchId))),
  reviewSamples: () => guarded(async () => unwrap(await commands.reviewSampleList())),
  rules: () => guarded(async () => unwrap(await commands.ruleList())),
  auditRun: () => guarded(async () => unwrap(await commands.auditRun())),
  issues: (status: string | null = "open") =>
    guarded(async () => unwrap(await commands.issueList(status))),
  issueAction: (input: IssueActionInput) =>
    guarded(async () => unwrap(await commands.issueAction(input))),
  draftAcceptVariant: (
    candidateId: string,
    chapterId: string,
    useRefined: boolean,
    acknowledgeFactChanges: boolean,
  ) =>
    guarded(async () =>
      unwrap(
        await commands.draftAcceptVariant(
          candidateId,
          chapterId,
          useRefined,
          acknowledgeFactChanges,
        ),
      ),
    ),
  revisions: (chapterId: string | null) =>
    guarded(async () => unwrap(await commands.revisionList(chapterId))),
  revisionCompare: (fromId: string, toId: string) =>
    guarded(async () => unwrap(await commands.revisionCompare(fromId, toId))),
  revisionRollback: (revisionId: string) =>
    guarded(async () => unwrap(await commands.revisionRollback(revisionId))),
  preferenceSave: (input: PreferenceInput) =>
    guarded(async () => unwrap(await commands.preferenceSave(input))),
  branches: () => guarded(async () => unwrap(await commands.branchList())),
  branchCreate: (name: string, fromBranchId: string | null = null) =>
    guarded(async () => unwrap(await commands.branchCreate(name, fromBranchId))),
  branchSwitch: (id: string) =>
    guarded(async () => unwrap(await commands.branchSwitch(id))),
  timeline: (branchId: string | null = null) =>
    guarded(async () => unwrap(await commands.timelineData(branchId))),
  graphData: () => guarded(async () => unwrap(await commands.graphData())),
  communityBuild: () => guarded(async () => unwrap(await commands.communityBuild())),
  communities: () => guarded(async () => unwrap(await commands.communityList())),
  communitySummarize: () =>
    guarded(async () => unwrap(await commands.communitySummarize())),
  impactAnalysis: (entityId: string) =>
    guarded(async () => unwrap(await commands.impactAnalysis(entityId))),
  insightQuery: (question: string) =>
    guarded(async () => unwrap(await commands.insightQuery(question))),
  wikiList: () => guarded(async () => unwrap(await commands.wikiList())),
  wikiRebuild: () => guarded(async () => unwrap(await commands.wikiRebuild())),
  wikiDependencies: () =>
    guarded(async () => unwrap(await commands.wikiDependencies())),
  wikiCompileLlm: (limit: number | null = 4) =>
    guarded(async () => unwrap(await commands.wikiCompileLlm(limit))),
  wikiProposeEdit: (input: WikiEditInput) =>
    guarded(async () => unwrap(await commands.wikiProposeEdit(input))),
  voiceSamples: (characterId: string) =>
    guarded(async () => unwrap(await commands.voiceSampleList(characterId))),
  voiceSampleAdd: (input: VoiceSampleInput) =>
    guarded(async () => unwrap(await commands.voiceSampleAdd(input))),
  voiceProfile: (characterId: string) =>
    guarded(async () => unwrap(await commands.voiceProfileGet(characterId))),
  voiceThresholdsSave: (input: VoiceThresholdInput) =>
    guarded(async () => unwrap(await commands.voiceThresholdsSave(input))),
  voiceCheck: (characterId: string, text: string, sceneId: string | null = null) =>
    guarded(async () => unwrap(await commands.voiceCheck(characterId, text, sceneId))),
  preferenceSummary: (sceneId: string | null = null) =>
    guarded(async () => unwrap(await commands.preferenceSummary(sceneId))),
  motifList: () => guarded(async () => unwrap(await commands.motifList())),
  motifDensityCheck: (sceneId: string, text: string) =>
    guarded(async () => unwrap(await commands.motifDensityCheck(sceneId, text))),
  exportProject: (input: ExportInput) =>
    guarded(async () => unwrap(await commands.exportProject(input))),

  agentTools: () => guarded(async () => commands.agentTools()),
  agentRuns: (limit: number | null = 40) =>
    guarded(async () => unwrap(await commands.agentRuns(limit))),
  agentRunCreate: (input: AgentRunCreateInput) =>
    guarded(async () => unwrap(await commands.agentRunCreate(input))),
  agentRunGet: (id: string) =>
    guarded(async () => unwrap(await commands.agentRunGet(id))),
  agentRunExecute: (id: string) =>
    guarded(async () => unwrap(await commands.agentRunExecute(id))),
  agentRunPause: (id: string) =>
    guarded(async () => unwrap(await commands.agentRunPause(id))),
  agentRunCancel: (id: string) =>
    guarded(async () => unwrap(await commands.agentRunCancel(id))),
  agentRunResume: (id: string) =>
    guarded(async () => unwrap(await commands.agentRunResume(id))),
  agentRunRetryStep: (runId: string, stepId: string) =>
    guarded(async () => unwrap(await commands.agentRunRetryStep(runId, stepId))),
  agentFoundation: () =>
    guarded(async () => unwrap(await commands.agentFoundation())),
  agentNextActions: () =>
    guarded(async () => unwrap(await commands.agentNextActions())),
  agentStyleProposal: (sessionId: string, instruction: string) =>
    guarded(async () => unwrap(await commands.agentStyleProposal(sessionId, instruction))),

  embeddingStatus: () =>
    guarded(async () => unwrap(await commands.embeddingStatus())),
  embeddingTest: () =>
    guarded(async () => unwrap(await commands.embeddingTest())),
  embeddingIndex: (rebuild = false) =>
    guarded(async () => unwrap(await commands.embeddingIndex(rebuild))),
  embeddingCancelIndex: () =>
    guarded(async () => unwrap(await commands.embeddingCancelIndex())),
  hybridSearch: (query: string, limit: number | null = 12) =>
    guarded(async () => unwrap(await commands.hybridSearch(query, limit))),
};

export type ChatDeltaEvent = { sessionId: string; requestId: string; delta: string };

export async function onChatDelta(
  handler: (event: ChatDeltaEvent) => void,
): Promise<() => void> {
  if (!isTauri()) return () => {};
  return listen<ChatDeltaEvent>("chat://delta", (event) => handler(event.payload));
}

export type GenerationDeltaEvent = { runId: string; delta: string };
export type GenerationFinishedEvent = {
  runId: string;
  candidateId: string | null;
  status: string;
};
export type GenerationErrorEvent = { runId: string; error: string };

export async function onGenerationDelta(
  handler: (event: GenerationDeltaEvent) => void,
): Promise<() => void> {
  if (!isTauri()) return () => {};
  return listen<GenerationDeltaEvent>("generation://delta", (event) => handler(event.payload));
}

export async function onGenerationFinished(
  handler: (event: GenerationFinishedEvent) => void,
): Promise<() => void> {
  if (!isTauri()) return () => {};
  return listen<GenerationFinishedEvent>("generation://finished", (event) =>
    handler(event.payload),
  );
}

export async function onGenerationError(
  handler: (event: GenerationErrorEvent) => void,
): Promise<() => void> {
  if (!isTauri()) return () => {};
  return listen<GenerationErrorEvent>("generation://error", (event) => handler(event.payload));
}

export type AgentProgressEvent = {
  runId: string;
  status: string;
  currentStep: number;
  totalSteps: number;
  message: string;
};

export async function onAgentProgress(
  handler: (event: AgentProgressEvent) => void,
): Promise<() => void> {
  if (!isTauri()) return () => {};
  return listen<AgentProgressEvent>("agent://progress", (event) => handler(event.payload));
}

export async function onEmbeddingProgress(
  handler: (event: EmbeddingProgressEvent) => void,
): Promise<() => void> {
  if (!isTauri()) return () => {};
  return listen<EmbeddingProgressEvent>("embedding://progress", (event) =>
    handler(event.payload),
  );
}

export type EmbeddingProgressEvent = {
  indexed: number;
  total: number;
  model: string;
  message: string;
};

export { commands };
