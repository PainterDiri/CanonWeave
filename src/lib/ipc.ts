import { listen } from "@tauri-apps/api/event";
import {
  commands,
  type CandidateCaptureInput,
  type ChatSessionCreateInput,
  type CharacterInput,
  type ModelConfig,
  type OutlineUpdateInput,
  type PremiseInput,
  type PromiseInput,
  type PromiseStateInput,
  type ProjectSnapshot,
} from "./bindings";

export type {
  Candidate,
  ChatMessage,
  ChatSession,
  CharacterDetail,
  ModelConfig,
  OutlineNode,
  OutlineUpdateInput,
  Premise,
  PremiseInput,
  CharacterInput,
  PromiseInput,
  PromiseStateInput,
  ChatSessionCreateInput,
  CandidateCaptureInput,
  PromiseRecord,
  ProjectSnapshot,
  ReviewQueue,
  SourceDocument,
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
  embeddingModel: null,
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
    embeddingModel: string | null;
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

  seedGenerate: (logline: string) =>
    guarded(async () => unwrap(await commands.seedGenerate({ logline }))),
  premiseList: () => guarded(async () => unwrap(await commands.premiseList())),
  premiseActive: () => guarded(async () => unwrap(await commands.premiseActive())),
  premiseUpdate: (id: string, input: PremiseInput) =>
    guarded(async () => unwrap(await commands.premiseUpdate(id, input))),
  premiseConfirm: (id: string) =>
    guarded(async () => unwrap(await commands.premiseConfirm(id))),
  premiseDebugSources: (id: string) =>
    guarded(async () => unwrap(await commands.premiseDebugSources(id))),

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

  outlineList: () => guarded(async () => unwrap(await commands.outlineList())),
  outlineGenerate: () => guarded(async () => unwrap(await commands.outlineGenerate())),
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

  promiseList: () => guarded(async () => unwrap(await commands.promiseList())),
  promiseSave: (input: PromiseInput) =>
    guarded(async () => unwrap(await commands.promiseSave(input))),
  promiseTransition: (input: PromiseStateInput) =>
    guarded(async () => unwrap(await commands.promiseTransition(input))),
};

export type ChatDeltaEvent = { sessionId: string; requestId: string; delta: string };

export async function onChatDelta(
  handler: (event: ChatDeltaEvent) => void,
): Promise<() => void> {
  if (!isTauri()) return () => {};
  return listen<ChatDeltaEvent>("chat://delta", (event) => handler(event.payload));
}

export { commands };
