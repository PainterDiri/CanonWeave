import { test, expect } from "@playwright/test";
import path from "node:path";

const project = {
  project: {
    id: "demo",
    name: "雨夜的第七封信",
    rootPath: "/Users/writer/Stories/rain-night",
    mode: "both",
    activeBranchId: "main-demo",
    writingLanguage: "zh-CN",
    schemaVersion: 1,
    createdAt: "2026-10-20T08:00:00Z",
    updatedAt: "2026-10-20T08:00:00Z",
  },
  counts: {
    premises: 1,
    sessions: 1,
    pendingCandidates: 6,
    chapters: 10,
    entities: 4,
    knowledgeEntries: 4,
    openIssues: 1,
    promises: 3,
  },
};

const premise = {
  id: "p1",
  logline: "父亲葬礼上，所有人都收到了他寄自下周的信。",
  genre: "悬疑",
  subgenre: "家庭悬疑",
  theme: "记忆是否真的能替人作证",
  emotionalPromise: "层层收紧的怀疑，与迟来的理解",
  tone: "潮湿、克制、略带压迫",
  coreConflict: "林澈想证明父亲的死并非意外，而母亲坚持让葬礼按计划结束。",
  dramaticQuestion: "林澈能否在不抹掉父亲真实死因的情况下找到寄信者？",
  antagonisticForce: "守夜人通过旧车站规则封锁所有能证明死亡的记录。",
  worldRule: "未来信每提前出现一次，就会从寄信者的记忆里消失一天。",
  endingDirection: "林澈接受父亲已死，但改变了家族对死亡的处理方式。",
  protagonistDesire: "找到信件的寄出者",
  flaw: "只相信可核对的证据",
  secret: "他曾在三年前伪造过父亲的一封信",
  worldHook: ["雨夜旧车站只在特定日期出现", "信件会提前七天寄达"],
  pov: "第三人称限知",
  person: "第三人称",
  tense: "过去时",
  targetLength: "80 万字",
  updateRhythm: "日更 4000 字",
  endingIntent: "开放",
  taboo: ["不写灵异实锤", "不用梦境解释一切"],
  status: "confirmed",
  sourceMessageId: "m1",
  fieldSources: { logline: "author", coreConflict: "author", tone: "inferred" },
  version: 3,
  createdAt: "2026-10-20T08:00:00Z",
  updatedAt: "2026-10-20T08:00:00Z",
};

const candidate = {
  id: "c1",
  kind: "claim",
  payload: {
    subject: "林澈",
    predicate: "持有旧钥匙",
    object: "黄铜钥匙",
    truthLayer: "world",
    quote: "林澈把钥匙攥进掌心，铜齿压出红痕。",
  },
  sourceSpan: "import:ch01:chapter:1｜林澈把钥匙攥进掌心",
  sourceMessageId: "m2",
  confidence: 0.82,
  riskLevel: "L2",
  status: "pending",
  batchId: null,
  createdAt: "2026-10-20T08:10:00Z",
  confirmedAt: null,
};

const candidate2 = {
  ...candidate,
  id: "c2",
  kind: "entity",
  payload: { type: "character", canonicalName: "林澈", aliases: ["阿澈"], description: "主角，负责葬礼的流程。" },
  riskLevel: "L1",
};

const chapter = (n: number, title: string, status = "candidate") => ({
  id: `o${n}`,
  number: n,
  volumeId: n <= 5 ? "v1" : "v2",
  volumeTitle: n <= 5 ? "第一卷 · 雨中的来信" : "第二卷 · 空站的回声",
  currentRevisionId: n <= 2 ? `rev-${n}` : null,
  styleProfileId: "style1",
  styleEpoch: 1,
  sceneCount: n === 1 ? 1 : 0,
  updatedAt: "2026-10-20T08:05:00Z",
  parentId: n <= 5 ? "v1" : "v2",
  level: "chapter",
  orderNo: n,
  title,
  goal: ["拿到那封下周寄出的信", "确认钥匙属于谁", "在守夜人面前隐藏父亲的秘密", "找到旧车站的入口"][n - 1] ?? "推进主线",
  conflict: "母亲要求立刻结束葬礼，守夜人拒绝交出钥匙。",
  turn: "信的日期又向前跳了一天。",
  payoff: "回收旧钥匙；新增车站的检票口。",
  logicNodeIds: ["n1"],
  presentationMode: "prose",
  narrativeFunction: n === 1 ? "inciting" : "complication",
  status,
  branchId: null,
  source: "ai",
  createdAt: "2026-10-20T08:05:00Z",
  updatedAt: "2026-10-20T08:05:00Z",
});

async function installMock(page: import("@playwright/test").Page) {
  await page.addInitScript(
    ({ project, premise, candidate, candidate2, chapters }) => {
      let callbackId = 1;
      let agentStatus = "running";
      let agentCandidates = [
        {
          id: "ac1",
          kind: "entity",
          payload: {
            type: "character",
            canonicalName: "守夜人",
            aliases: [],
            description: "掌握旧车站规则的对手。",
          },
          sourceSpan: "import:ch01｜守夜人看见钥匙时退了一步。",
          sourceMessageId: null,
          confidence: 0.86,
          riskLevel: "L1",
          status: "pending",
          batchId: "run1",
          agentRunId: "run1",
          agentStepId: "step3",
          createdAt: "2026-10-20T08:10:00Z",
          confirmedAt: null,
        },
        {
          id: "ac2",
          kind: "knowledge_entry",
          payload: {
            domainId: "domain-env",
            name: "雨夜旧车站",
            summary: "只在特定雨夜出现，钟比现实慢七分钟。",
            fields: { rules: "错过末班车会被困到下一次雨夜。" },
          },
          sourceSpan: "chapter:o1｜旧车站的钟比现实慢七分钟。",
          sourceMessageId: null,
          confidence: 0.78,
          riskLevel: "L2",
          status: "pending",
          batchId: "run1",
          agentRunId: "run1",
          agentStepId: "step3",
          createdAt: "2026-10-20T08:11:00Z",
          confirmedAt: null,
        },
      ];
      const agentSteps = [
        {
          id: "step1",
          runId: "run1",
          seq: 1,
          kind: "tool",
          toolName: "project_inventory",
          input: {},
          output: {
            counts: { entities: 4, knowledgeEntries: 4 },
            evidenceSources: [{ quote: "守夜人看见钥匙时退了一步。" }],
          },
          status: "completed",
          riskLevel: "L0",
          requiresApproval: false,
          candidateIds: [],
          evidence: [],
          error: "",
          startedAt: "2026-10-20T08:10:00Z",
          finishedAt: "2026-10-20T08:10:01Z",
        },
        {
          id: "step2",
          runId: "run1",
          seq: 2,
          kind: "tool",
          toolName: "search_fts",
          input: { query: "整理项目资料" },
          output: { query: "整理项目资料", hits: [{}, {}], evidenceSources: [] },
          status: "completed",
          riskLevel: "L0",
          requiresApproval: false,
          candidateIds: [],
          evidence: [],
          error: "",
          startedAt: "2026-10-20T08:10:01Z",
          finishedAt: "2026-10-20T08:10:02Z",
        },
        {
          id: "step3",
          runId: "run1",
          seq: 3,
          kind: "synthesis",
          toolName: "model_synthesis",
          input: {},
          output: {
            summary: "识别人物与知识条目候选。",
            created: [{ candidateId: "ac1" }, { candidateId: "ac2" }],
            skipped: [],
          },
          status: "completed",
          riskLevel: "L2",
          requiresApproval: true,
          candidateIds: ["ac1", "ac2"],
          evidence: [],
          error: "",
          startedAt: "2026-10-20T08:10:02Z",
          finishedAt: "2026-10-20T08:10:08Z",
        },
        {
          id: "step4",
          runId: "run1",
          seq: 4,
          kind: "check",
          toolName: "foundation_check",
          input: {},
          output: { ready: false, passedCount: 5, totalCount: 7, errorCount: 2 },
          status: "completed",
          riskLevel: "L0",
          requiresApproval: false,
          candidateIds: [],
          evidence: [],
          error: "",
          startedAt: "2026-10-20T08:10:08Z",
          finishedAt: "2026-10-20T08:10:09Z",
        },
      ];
      const agentRun = (status = agentStatus, goal = "分析当前项目并整理人物与知识候选") => ({
        id: "run1",
        sessionId: null,
        mode: "propose",
        scope: "project",
        output: "all",
        goal,
        status,
        plan: [],
        currentStep: 4,
        approvalPolicy: "manual",
        model: "gpt-4o-mini",
        contextHash: "ctx1",
        error: "",
        startedAt: "2026-10-20T08:10:00Z",
        finishedAt: null,
        createdAt: "2026-10-20T08:09:00Z",
        updatedAt: "2026-10-20T08:10:09Z",
        pendingCandidates: agentCandidates.filter((item) => item.status === "pending").length,
        totalCandidates: agentCandidates.length,
      });
      const agentDetail = (status = agentStatus, goal?: string) => ({
        run: agentRun(status, goal),
        steps: agentSteps,
        candidates: agentCandidates,
        evidence: [
          {
            id: "ae1",
            runId: "run1",
            stepId: "step3",
            sourceKind: "import",
            sourceId: "ch01",
            sourceSpan: "import:ch01",
            quote: "守夜人看见钥匙时退了一步。",
            createdAt: "2026-10-20T08:10:05Z",
          },
        ],
      });
      (window as any).__TAURI_INTERNALS__ = {
        transformCallback: () => callbackId++,
        invoke: async (cmd: string, args?: any) => {
          switch (cmd) {
            case "project_state":
              return project;
            case "model_get":
              return {
                baseUrl: "https://api.openai.com/v1",
                model: "gpt-4o-mini",
                temperature: 0.8,
                maxTokens: 4096,
                embeddingModel: null,
                hasApiKey: true,
              };
            case "premise_list":
              return [premise];
            case "premise_active":
              return premise;
            case "blueprint_update": {
              const input = args.input;
              const edges = input.edges.map((edge: any, index: number) => ({
                ...edge,
                id: edge.id || `edge-new-${index}`,
              }));
              return {
                blueprint: {
                  id: input.id,
                  premiseId: "p1",
                  title: input.title,
                  medium: input.medium,
                  structureStrategy: input.structureStrategy,
                  targetUnits: input.targetUnits,
                  coreDramaticQuestion: input.coreDramaticQuestion,
                  narrativePromise: input.narrativePromise,
                  openingState: input.openingState,
                  endingState: input.endingState,
                  themeQuestion: input.themeQuestion,
                  status: "candidate",
                  completeness: edges.length > 0 ? 92 : 70,
                  version: 2,
                  createdAt: "2026-10-20T08:05:00Z",
                  updatedAt: "2026-10-20T08:05:00Z",
                },
                elements: input.elements,
                timeline: input.timeline,
                threads: input.threads,
                nodes: input.nodes,
                edges,
                presentations: input.presentations,
                diagnostics: edges.length > 0 ? [] : [{
                  code: "graph.disconnected",
                  severity: "error",
                  message: "因果链存在断点，无法从起点完整推到终局",
                  path: "edges",
                }],
              };
            }
            case "blueprint_current":
            case "blueprint_candidate":
              return {
                blueprint: {
                  id: "bp1",
                  premiseId: "p1",
                  title: "雨夜来信",
                  medium: "novel",
                  structureStrategy: "adaptive",
                  targetUnits: null,
                  coreDramaticQuestion: "林澈能否在不改写父亲死亡的前提下找到寄信者？",
                  narrativePromise: "每次接近真相都会让一封未来的信提前抵达。",
                  openingState: "所有人相信父亲已死。",
                  endingState: "林澈接受死亡不可逆，但改变了自己与父亲的关系。",
                  themeQuestion: "人是否能通过记忆改变已经发生的事？",
                  status: "candidate",
                  completeness: 92,
                  version: 1,
                  createdAt: "2026-10-20T08:05:00Z",
                  updatedAt: "2026-10-20T08:05:00Z",
                },
                elements: [
                  { id: "e1", kind: "character", name: "林澈", role: "主角", summary: "在葬礼上收到未来来信", externalGoal: "找到寄信者", internalNeed: "接受父亲已经死亡", fear: "再次失去家人", flaw: "把控制当成爱", leverage: "能辨认父亲的字迹", constraints: [], secret: "曾改过父亲遗书", relations: [], metadata: {}, status: "confirmed", sortOrder: 0 },
                  { id: "e2", kind: "character", name: "守夜人", role: "对手", summary: "知道旧车站规则的人", externalGoal: "维持车站", internalNeed: "赎罪", fear: "时间规则崩塌", flaw: "隐瞒", leverage: "控制车站", constraints: [], secret: "父亲留下规则", relations: [], metadata: {}, status: "confirmed", sortOrder: 1 },
                  { id: "e3", kind: "world_rule", name: "未来信", role: "世界规则", summary: "信只能在葬礼后出现", externalGoal: "", internalNeed: "", fear: "", flaw: "", leverage: "", constraints: [], secret: "", relations: [], metadata: {}, status: "confirmed", sortOrder: 2 },
                ],
                timeline: [
                  { id: "t1", orderNo: 1, title: "父亲进入车站", storyTime: "十年前", cause: "寻找失踪者", participants: ["父亲"], outcome: "父亲失踪", consequence: "林澈开始守夜", revealAt: "开篇" },
                  { id: "t2", orderNo: 2, title: "未来信寄出", storyTime: "现在", cause: "旧车站重启", participants: ["父亲", "林澈"], outcome: "林澈收到信", consequence: "葬礼被打断", revealAt: "第一章" },
                  { id: "t3", orderNo: 3, title: "守夜人现身", storyTime: "现在", cause: "信被打开", participants: ["守夜人", "林澈"], outcome: "规则暴露", consequence: "林澈必须做出选择", revealAt: "第一卷末" },
                ],
                threads: [
                  { id: "th1", orderNo: 1, kind: "main", title: "未来信主线", dramaticQuestion: "谁在寄信？", openingState: "葬礼", targetState: "真相揭示", stakes: "林澈的家庭记忆", status: "confirmed" },
                ],
                nodes: [
                  { id: "n1", threadId: "th1", threadTitle: "未来信主线", orderNo: 1, kind: "inciting", title: "未来信出现", cause: "父亲葬礼", choice: "林澈打开信", cost: "葬礼被打断", outcome: "信件日期暴露", stateChange: "家族秘密浮出", knowledgeChange: "林澈知道父亲留下规则", participants: ["林澈"], location: "殡仪馆", storyTime: "现在", status: "confirmed" },
                  { id: "n2", threadId: "th1", threadTitle: "未来信主线", orderNo: 2, kind: "reveal", title: "车站规则", cause: "林澈打开信", choice: "前往旧车站", cost: "失去父亲遗物", outcome: "守夜人现身", stateChange: "林澈被卷入", knowledgeChange: "守夜人知道寄信者", participants: ["林澈", "守夜人"], location: "旧车站", storyTime: "当夜", status: "confirmed" },
                ],
                edges: [
                  { id: "edge1", fromNodeId: "n1", fromTitle: "未来信出现", toNodeId: "n2", toTitle: "车站规则", relation: "导致", condition: "林澈未交出信", status: "confirmed" },
                ],
                presentations: [],
                diagnostics: [],
              };
            case "outline_list":
              return [
                {
                  id: "v1",
                  parentId: null,
                  level: "volume",
                  orderNo: 1,
                  title: "第一卷 · 雨中的来信",
                  goal: "确认父亲死亡背后的异常",
                  conflict: "家族要求尽快结束葬礼",
                  turn: "信件来自尚未发生的下周",
                  payoff: "打开旧车站",
                  logicNodeIds: [],
                  presentationMode: "prose",
                  narrativeFunction: "",
                  status: "confirmed",
                  branchId: null,
                  source: "ai",
                  createdAt: "2026-10-20T08:05:00Z",
                  updatedAt: "2026-10-20T08:05:00Z",
                },
                ...chapters.slice(0, 5).map((item) => ({ ...item, parentId: "v1", status: "confirmed" })),
                {
                  id: "v2",
                  parentId: null,
                  level: "volume",
                  orderNo: 2,
                  title: "第二卷 · 空站的回声",
                  goal: "追查寄信者与旧车站",
                  conflict: "母亲和守夜人各自隐瞒真相",
                  turn: "父亲留下的是一套未完成的时间规则",
                  payoff: "把选择交给林澈",
                  logicNodeIds: [],
                  presentationMode: "prose",
                  narrativeFunction: "",
                  status: "candidate",
                  branchId: null,
                  source: "ai",
                  createdAt: "2026-10-20T08:05:00Z",
                  updatedAt: "2026-10-20T08:05:00Z",
                },
                ...chapters.slice(5).map((item) => ({ ...item, parentId: "v2" })),
              ];
            case "chat_session_list":
              return [
                {
                  id: "s1",
                  scope: "seed",
                  sceneId: null,
                  branchId: null,
                  parentSessionId: null,
                  mode: "diverge",
                  title: "葬礼上的信",
                  model: "gpt-4o-mini",
                  status: "active",
                  summary: "",
                  lastMessageAt: "2026-10-20T08:20:00Z",
                  createdAt: "2026-10-20T08:00:00Z",
                  pinned: true,
                  archivedAt: null,
                  pendingCandidates: 2,
                  messageCount: 2,
                },
              ];
            case "chat_messages_list":
              return [
                {
                  id: "m1",
                  sessionId: "s1",
                  seq: 1,
                  role: "user",
                  content: "父亲葬礼上，所有人都收到了他寄自下周的信。",
                  model: "",
                  tokens: 0,
                  createdAt: "2026-10-20T08:01:00Z",
                  linkedCandidateIds: [],
                  pinned: true,
                },
                {
                  id: "m2",
                  sessionId: "s1",
                  seq: 2,
                  role: "assistant",
                  content:
                    "建议把第一场放在守夜之后：母亲坚持封棺，林澈却在父亲的外套里摸到一枚不属于任何人的黄铜钥匙。守夜人看见钥匙时退了一步——他知道些什么。\n\n这不是正文，只是一个可选的冲突节拍。",
                  model: "gpt-4o-mini",
                  tokens: 210,
                  createdAt: "2026-10-20T08:02:00Z",
                  linkedCandidateIds: [],
                  pinned: false,
                },
              ];
            case "chat_candidates":
              return [candidate, candidate2];
            case "agent_tools":
              return [
                {
                  name: "project_inventory",
                  description: "读取项目清单",
                  parameterSchema: {},
                  access: "read",
                  riskLevel: "L0",
                  requiresApproval: false,
                  retryable: true,
                  outputTarget: "context",
                },
                {
                  name: "propose_entity",
                  description: "提出实体候选",
                  parameterSchema: {},
                  access: "write",
                  riskLevel: "L1",
                  requiresApproval: true,
                  retryable: true,
                  outputTarget: "candidate",
                },
              ];
            case "agent_runs":
              return [agentRun()];
            case "agent_run_get":
              return agentDetail();
            case "agent_run_create":
              agentStatus = "running";
              return agentDetail("draft", args?.input?.goal);
            case "agent_run_execute":
              agentStatus = "waiting_approval";
              return agentDetail();
            case "agent_run_pause":
              agentStatus = "paused";
              return agentDetail();
            case "agent_run_resume":
              agentStatus = "running";
              return agentDetail();
            case "agent_run_cancel":
              agentStatus = "canceled";
              return agentDetail();
            case "agent_run_retry_step":
              agentStatus = "running";
              return agentDetail();
            case "agent_foundation":
              return {
                ready: false,
                passedCount: 5,
                totalCount: 7,
                errorCount: 2,
                warningCount: 0,
                checks: [
                  {
                    code: "foundation.core_cast",
                    label: "核心人物与对手",
                    severity: "pass",
                    message: "已具备两个核心角色。",
                    evidence: [],
                  },
                  {
                    code: "foundation.relationship_tension",
                    label: "关系张力",
                    severity: "error",
                    message: "核心人物之间还没有写明目标冲突。",
                    evidence: [],
                  },
                  {
                    code: "foundation.timeline_anchors",
                    label: "时间线锚点",
                    severity: "error",
                    message: "时间锚点不足三个。",
                    evidence: [],
                  },
                ],
              };
            case "candidate_accept":
              agentCandidates = agentCandidates.map((item) =>
                item.id === args?.id ? { ...item, status: "accepted" } : item,
              );
              return {
                candidate: agentCandidates.find((item) => item.id === args?.id),
                createdId: "new-entity",
              };
            case "candidate_reject":
              agentCandidates = agentCandidates.map((item) =>
                item.id === args?.id ? { ...item, status: "rejected" } : item,
              );
              return agentCandidates.find((item) => item.id === args?.id);
            case "candidate_list":
              return args?.kind === "draft" || args?.kind === "wiki_edit"
                ? []
                : [candidate, candidate2];
            case "chapter_list":
              return chapters;
            case "branch_list":
              return [
                {
                  id: "main-demo",
                  name: "主线",
                  parentBranchId: null,
                  status: "active",
                  createdAt: "2026-10-20T08:00:00Z",
                  isCurrent: true,
                },
              ];
            case "timeline_data":
              return [
                {
                  id: "ev1",
                  kind: "event",
                  label: "葬礼当夜",
                  storyTime: "第1日 23:00",
                  revealTime: "第1章",
                  branchId: null,
                  source: "ch01:p1",
                  detail: "林澈收到下周寄出的信",
                },
                {
                  id: "ev2",
                  kind: "event",
                  label: "旧车站开启",
                  storyTime: "第2日 02:00",
                  revealTime: "第4章",
                  branchId: "main-demo",
                  source: "ch04:p11",
                  detail: "母亲承认曾经去过车站",
                },
              ];
            case "graph_data":
              return {
                nodes: [
                  { id: "e1", label: "林澈", kind: "character", community: 0 },
                  { id: "e2", label: "母亲", kind: "character", community: 0 },
                  { id: "e3", label: "旧车站", kind: "place", community: 0 },
                ],
                edges: [
                  { id: "r1", from: "e1", to: "e2", label: "母子" },
                  { id: "r2", from: "e2", to: "e3", label: "守夜人" },
                ],
              };
            case "community_list":
              return [
                {
                  id: "comm1",
                  level: 0,
                  title: "关系簇 · 林澈 / 母亲 / 旧车站",
                  memberIds: ["e1", "e2", "e3"],
                  sourceIds: ["ch01:p1", "ch04:p11"],
                  content: "母亲与旧车站之间存在尚未解释的联系；林澈的调查正在把家庭秘密推向公开。",
                  summaryHash: "hash1",
                  generatedAt: "2026-10-20T08:00:00Z",
                  staleAt: null,
                },
              ];
            case "chapter_text_get":
              return "雨落在旧灯上。\n\n林澈把信摊在掌心，纸页的边缘被水汽泡得发软。母亲站在门口，没有进来，也没有说话。\n\n他听见楼下有人在喊父亲的名字。";
            case "scene_list":
              return [
                {
                  id: "sc1",
                  chapterId: "o1",
                  number: 1,
                  goal: "在封棺前拿到那封下周寄出的信",
                  conflict: "母亲坚持立刻结束葬礼，守夜人拒绝让路。",
                  pov: "林澈",
                  storyTime: "葬礼当夜，十一点",
                  branchId: null,
                  placeId: null,
                  styleProfileId: "style1",
                  emotionBeat: {
                    target: "压抑",
                    intensity: 4,
                    entry: "麻木",
                    exit: "动摇",
                    turn: "信纸是空白的",
                  },
                  sensoryPlan: {
                    channels: ["嗅觉", "触觉"],
                    motifs: ["雨", "旧纸"],
                    distance: "近景",
                  },
                  status: "planned",
                  createdAt: "2026-10-20T08:00:00Z",
                  updatedAt: "2026-10-20T08:00:00Z",
                },
              ];
            case "style_profile_list":
              return [
                {
                  id: "style1",
                  scope: "project",
                  scopeId: null,
                  name: "冷峻克制",
                  pov: "第三人称限知",
                  person: "第三人称",
                  tense: "过去时",
                  syntax: {
                    avgSentenceMin: 16,
                    avgSentenceMax: 24,
                    shortRatio: 38,
                    paragraphSentencesMin: 3,
                    paragraphSentencesMax: 5,
                  },
                  lexicon: {
                    concreteRatio: 70,
                    register: "克制",
                    adverbDensityMax: 6,
                    bannedWords: ["仿佛", "似乎"],
                  },
                  imagery: {
                    sensoryChannels: ["视觉", "触觉"],
                    motifs: ["雨", "旧灯"],
                    sensoryPer1000: 8,
                  },
                  rhetoric: { metaphorPer1000: 2, irony: 10, minimalism: 60 },
                  emotion: { interiority: 45, externality: 55, intensityMin: 2, intensityMax: 4 },
                  ratios: { action: 35, dialogue: 18, description: 37, interiority: 10 },
                  antiStyle: ["总结式抒情", "解释动机"],
                  version: 2,
                  status: "confirmed",
                  createdAt: "2026-10-20T08:00:00Z",
                  updatedAt: "2026-10-20T08:00:00Z",
                },
              ];
            case "style_sample_list":
            case "style_preset_list":
              return [];
            case "style_contract_text":
              return "[STYLE CONTRACT v2]\n视角：第三人称限知，过去时\n句法：平均 16 至 24 字，短句比例约 38%\n意象：雨、旧灯\n禁例：总结式抒情、解释动机";
            case "generation_runs":
              return [];
            case "review_queue":
              return {
                candidates: [candidate, candidate2],
                counts: { l0: 0, l1: 1, l2: 1, l3: 0, total: 2 },
              };
            case "review_policy_get":
              return {
                id: "pol1",
                scope: "project",
                scopeId: null,
                mode: "balanced",
                autoAcceptLevel: "L0",
                sampleRate: 0,
                lockHard: true,
                updatedAt: "2026-10-20T08:00:00Z",
              };
            case "issue_list":
              return [
                {
                  id: "i1",
                  ruleId: "r1",
                  ruleCode: "dead_character_acts",
                  constraintLayer: "hard",
                  severity: "block",
                  claimIds: ["c1", "e1"],
                  evidenceSpans: ["ch03:p42-44｜父亲已经死亡", "ch09:p18-20｜林澈在雨里奔跑"],
                  explanation: "父亲在第 3 章死亡，但第 9 章以现实行动出现。",
                  certificate: {
                    ruleId: "dead_character_acts",
                    severity: "block",
                    claimIds: ["c1", "e1"],
                    evidenceSpans: ["ch03:p42-44", "ch09:p18-20"],
                    solverCore: ["claim:c1:dead", "event:e1:real_action", "rule:dead_character_acts"],
                    explanation: "父亲在第 3 章死亡，但第 9 章以现实行动出现。",
                    suggestedActions: ["修改正文", "标记为回忆", "建立分支"],
                  },
                  solverCore: ["claim:c1:dead", "event:e1:real_action", "rule:dead_character_acts"],
                  riskLevel: "L3",
                  fingerprint: "fp1",
                  status: "open",
                  deferredUntil: null,
                  createdAt: "2026-10-20T08:00:00Z",
                },
              ];
            case "rule_list":
              return [];
            case "revision_list":
              return [];
            case "character_list":
              return [
                {
                  entity: {
                    id: "e1",
                    entityType: "character",
                    canonicalName: "林澈",
                    aliases: ["阿澈"],
                    status: "confirmed",
                    description: "三十岁，纪录片剪辑师。习惯用证据安放情绪。",
                  },
                  desire: "证明父亲的死并非意外",
                  need: "允许自己承认对父亲的恨",
                  fear: "发现自己和父亲一样擅长撒谎",
                  flaw: "只相信可核对的证据",
                  values: ["诚实", "秩序"],
                  secret: "三年前伪造过父亲的一封信",
                  taboo: ["不拿母亲当筹码"],
                  voiceAnchor: "“先别下结论。把信给我。”",
                },
              ];
            case "knowledge_domains":
              return [
                {
                  id: "domain-env",
                  key: "environment",
                  name: "环境",
                  description: "地点之间的地理关系、空间布置、景色与环境规则。",
                  icon: "landmark",
                  accent: "teal",
                  isSystem: true,
                  sortOrder: 10,
                  entryCount: 1,
                  fieldSchema: [
                    { key: "category", label: "环境类型", placeholder: "城市、建筑群或特殊空间" },
                    { key: "geography", label: "地理关系", placeholder: "相邻、位于、如何抵达" },
                    { key: "landscape", label: "景色与感官", placeholder: "光线、声音、气味" },
                  ],
                  createdAt: "2026-10-20T08:00:00Z",
                  updatedAt: "2026-10-20T08:00:00Z",
                },
                {
                  id: "domain-item",
                  key: "item",
                  name: "重要物品",
                  description: "与剧情紧密相关的物品、能力、归属与流转记录。",
                  icon: "package",
                  accent: "teal",
                  isSystem: true,
                  sortOrder: 20,
                  entryCount: 1,
                  fieldSchema: [
                    { key: "function", label: "作用与能力", placeholder: "它能做什么" },
                    { key: "ownership", label: "归属与流转", placeholder: "当前持有者" },
                  ],
                  createdAt: "2026-10-20T08:00:00Z",
                  updatedAt: "2026-10-20T08:00:00Z",
                },
                {
                  id: "domain-forsaken",
                  key: "custom-forsaken",
                  name: "禁墟",
                  description: "记录禁墟能力、代价与克制关系。",
                  icon: "sparkles",
                  accent: "amber",
                  isSystem: false,
                  sortOrder: 30,
                  entryCount: 1,
                  fieldSchema: [
                    { key: "field1", label: "能力", placeholder: "禁墟能力" },
                    { key: "field2", label: "克制关系", placeholder: "被什么克制" },
                  ],
                  createdAt: "2026-10-20T08:00:00Z",
                  updatedAt: "2026-10-20T08:00:00Z",
                },
              ];
            case "knowledge_entries": {
              const rows = [
                {
                  id: "ke1",
                  entityId: "e3",
                  domainId: "domain-env",
                  domainKey: "environment",
                  domainName: "环境",
                  name: "旧车站",
                  aliases: ["雨站"],
                  summary: "只在雨夜出现，站台钟比现实慢七分钟。",
                  fields: {
                    category: "废弃车站",
                    geography: "位于北堤尽头，穿过旧检票口可达",
                    landscape: "潮湿铁轨、蓝色站灯、不断倒流的雨",
                  },
                  status: "confirmed",
                  createdAt: "2026-10-20T08:00:00Z",
                  updatedAt: "2026-10-20T08:00:00Z",
                },
                {
                  id: "ke2",
                  entityId: "e4",
                  domainId: "domain-forsaken",
                  domainKey: "custom-forsaken",
                  domainName: "禁墟",
                  name: "雨墟",
                  aliases: ["旧雨区"],
                  summary: "吞噬声音的空间。",
                  fields: { field1: "让所有声音延迟七分钟", field2: "雷火会烧穿边界" },
                  status: "confirmed",
                  createdAt: "2026-10-20T08:00:00Z",
                  updatedAt: "2026-10-20T08:00:00Z",
                },
              ];
              return rows.filter(
                (entry) => !args?.domainId || entry.domainId === args.domainId,
              );
            }
            case "voice_sample_list":
              return [
                {
                  id: "vs1",
                  characterId: "e1",
                  text: "林澈说：“先别下结论。把信给我。”",
                  sourceSpan: "作者样本 1",
                  sourceType: "author",
                  metrics: {
                    avgSentenceLen: 12,
                    sentenceVariance: 4,
                    punctuationPer100: 6,
                    pronounRatio: 4,
                    dialogueActionRatio: 30,
                    dialogueRatio: 48,
                    preferredPronoun: "我",
                  },
                  createdAt: "2026-10-20T08:00:00Z",
                },
              ];
            case "voice_profile_get":
              return {
                characterId: "e1",
                centroid: {
                  avgSentenceLen: 12,
                  sentenceVariance: 4,
                  punctuationPer100: 6,
                  pronounRatio: 4,
                  dialogueActionRatio: 30,
                  dialogueRatio: 48,
                  preferredPronoun: "我",
                },
                thresholds: {
                  sentenceLen: 10,
                  sentenceVariance: 12,
                  punctuation: 5,
                  pronoun: 6,
                  dialogueAction: 25,
                },
                sampleCount: 1,
                version: 1,
                updatedAt: "2026-10-20T08:00:00Z",
              };
            case "promise_list":
              return [
                {
                  id: "pr1",
                  title: "下周寄出的信",
                  state: "planted",
                  plantSceneId: null,
                  targetWindow: "第 3 至 5 章",
                  payoffContract: "揭示寄信人是谁，并让林澈意识到日期是伪造的。",
                  evidenceIds: ["import:ch01"],
                  createdAt: "2026-10-20T08:00:00Z",
                  updatedAt: "2026-10-20T08:00:00Z",
                },
                {
                  id: "pr2",
                  title: "黄铜钥匙",
                  state: "ready",
                  plantSceneId: null,
                  targetWindow: "第 2 章",
                  payoffContract: "打开父亲生前锁住的抽屉，改变林澈对父亲的判断。",
                  evidenceIds: ["import:ch01", "chat:s1:m2"],
                  createdAt: "2026-10-20T08:00:00Z",
                  updatedAt: "2026-10-20T08:00:00Z",
                },
              ];
            case "document_list":
              return [
                {
                  id: "d1",
                  path: "/story/imports/rain.docx",
                  hash: "abc",
                  docType: "docx",
                  title: "雨夜旧稿",
                  wordCount: 23400,
                  importedAt: "2026-10-20T08:00:00Z",
                },
              ];
            case "entity_list":
              return [];
            case "wiki_list":
              return [
                {
                  id: "wiki1",
                  entityId: "e1",
                  kind: "character",
                  title: "林澈",
                  content:
                    "派生视图 · 非权威事实。事实变更后本页会标记过期，并可全量重建。\n\n## 人物核心\n- 欲望：证明父亲的死并非意外\n- 恐惧：发现自己和父亲一样擅长撒谎\n\n## 已确认事实\n- [world] 林澈 · 持有 · 旧钥匙（ch01:p3）",
                  contentHash: "abcdef1234567890",
                  sourceIds: ["e1", "c1"],
                  sourceSpans: ["ch01:p3｜林澈握住旧钥匙"],
                  generatorVersion: 1,
                  editedByAuthor: false,
                  generatedAt: "2026-10-20T08:00:00Z",
                  staleAt: null,
                },
                {
                  id: "wiki2",
                  entityId: null,
                  kind: "timeline",
                  title: "双时间线",
                  content:
                    "派生视图 · 非权威事实。\n\n## 事件\n- 葬礼当夜｜故事时间第1日 23:00｜叙述时间第1章",
                  contentHash: "fedcba0987654321",
                  sourceIds: ["ev1"],
                  sourceSpans: ["ch01:p1｜林澈收到下周寄出的信"],
                  generatorVersion: 1,
                  editedByAuthor: false,
                  generatedAt: "2026-10-20T08:00:00Z",
                  staleAt: "2026-10-20T08:30:00Z",
                },
              ];
            case "preference_summary":
              return {
                choices: 1,
                target: {
                  avgSentenceLen: 14,
                  dialogueRatio: 24,
                  sensoryPer1000: 9,
                  metaphorPer1000: 2,
                  adverbPer1000: 5,
                },
              };
            case "motif_density_check":
              return {
                chars: 120,
                motifs: [],
                hints: [],
              };
            case "export_project":
              return {
                path: "/Users/writer/Stories/rain-night/exports/rain-night.md",
                format: args?.input?.format ?? "markdown",
                chapters: 10,
                words: 23400,
                includesWiki: true,
              };
            case "app_info":
              return { name: "CanonWeave", version: "0.1.0", schemaVersion: 6 };
            case "plugin:event|listen":
              return 1;
            default:
              return null;
          }
        },
      };
    },
    {
      project,
      premise,
      candidate,
      candidate2,
      chapters: Array.from({ length: 10 }, (_, index) =>
        chapter(index + 1, ["雨夜", "钥匙", "守夜人", "旧车站", "下周", "母亲", "检票口", "空信", "回声", "到站"][index], index < 3 ? "confirmed" : "candidate"),
      ),
    },
  );
}

test("workspace pages render with local fixture data", async ({ page }) => {
  await installMock(page);
  await page.goto("/");
  await expect(page.getByText("雨夜的第七封信").first()).toBeVisible();
  const primaryNav = page.locator("aside nav button");
  await expect(primaryNav.nth(1)).toContainText("Agent");
  await expect(primaryNav.nth(2)).toContainText("正文");
  await page.waitForTimeout(400);
  await page.screenshot({ path: path.join("test-results", "overview.png") });

  await page.getByRole("button", { name: "灵感与卷章", exact: true }).click();
  await expect(page.getByText("种子卡").first()).toBeVisible();
  await page.waitForTimeout(350);
  await page.screenshot({ path: path.join("test-results", "seed.png") });

  await page.getByRole("button", { name: /^Agent/ }).click();
  await expect(page.getByText("Agent 项目工作台")).toBeVisible();
  await expect(page.getByText("计划与执行轨迹")).toBeVisible();
  await expect(page.getByText("守夜人", { exact: true }).first()).toBeVisible();
  await page.getByRole("button", { name: "暂停" }).click();
  await expect(page.getByText("已暂停").first()).toBeVisible();
  await page.getByRole("button", { name: "恢复" }).click();
  await expect(page.getByText("待审批").first()).toBeVisible();
  await page.screenshot({ path: path.join("test-results", "agent.png") });

  await page.getByRole("button", { name: "新建任务" }).click();
  await page
    .getByPlaceholder("例如：分析现有正文，整理人物、关系和伏笔候选，并说明对当前骨架的影响。")
    .fill("从现有旧稿整理人物、关系和伏笔候选");
  await page.getByRole("button", { name: "启动任务" }).click();
  await expect(page.getByText("待审批", { exact: true }).first()).toBeVisible();
  await page.getByTitle("查看候选差异").first().click();
  await expect(page.getByText("结构化变更")).toBeVisible();
  await expect(page.getByText("来源证据")).toBeVisible();
  await page.getByRole("button", { name: "关闭" }).click();
  await page.getByTitle("确认候选").first().click();
  await expect(page.getByText("候选已确认并写入权威资料")).toBeVisible();

  await page.getByRole("button", { name: "自由讨论" }).click();
  await expect(page.getByText("建议把第一场放在守夜之后")).toBeVisible();
  await page.screenshot({ path: path.join("test-results", "chat.png") });

  await page.getByRole("button", { name: "灵感与卷章", exact: true }).click();
  await page.getByRole("button", { name: "叙事蓝本", exact: true }).click();
  await expect(page.getByText("叙事逻辑链")).toBeVisible();
  await page.getByRole("button", { name: "编辑 林澈", exact: true }).click();
  await expect(page.getByText("编辑人物 · 林澈")).toBeVisible();
  await page.getByRole("button", { name: "关闭", exact: true }).click();
  await page.getByRole("button", { name: "新增因果边", exact: true }).click();
  await page.getByRole("button", { name: "逻辑节点 #2 车站规则", exact: true }).click();
  await page.getByRole("button", { name: "逻辑节点 #1 未来信出现", exact: true }).click();
  await expect(page.getByRole("heading", { name: "新增因果边", exact: true })).toBeVisible();
  await page.getByRole("button", { name: "保存并重新诊断", exact: true }).click();
  await expect(page.getByText("2 条因果边", { exact: false })).toBeVisible();
  await page.getByTestId("logic-edge-edge-new-1").click();
  await page.getByRole("button", { name: "删除此边", exact: true }).click();
  await expect(page.getByText("1 条因果边", { exact: false })).toBeVisible();
  await page.waitForTimeout(350);
  await page.screenshot({ path: path.join("test-results", "blueprint.png") });
  await page.getByRole("button", { name: "卷章编排", exact: true }).click();
  await page.waitForTimeout(350);
  await page.screenshot({ path: path.join("test-results", "outline.png") });

  await page.getByRole("button", { name: /审计/ }).click();
  await page.waitForTimeout(350);
  await page.screenshot({ path: path.join("test-results", "audit.png") });

  await page.getByRole("button", { name: "人物", exact: true }).click();
  await expect(page.getByText("人物声音指纹")).toBeVisible();
  await page.waitForTimeout(350);
  await page.screenshot({ path: path.join("test-results", "characters.png") });

  await page.getByRole("button", { name: "知识库", exact: true }).click();
  await expect(page.getByText("环境").first()).toBeVisible();
  await expect(page.getByText("旧车站").first()).toBeVisible();
  await page.waitForTimeout(350);
  await page.screenshot({ path: path.join("test-results", "knowledge.png") });

  await page.getByRole("button", { name: "伏笔", exact: true }).click();
  await page.waitForTimeout(350);
  await page.screenshot({ path: path.join("test-results", "promises.png") });

  await page.getByRole("button", { name: "叙事百科", exact: true }).click();
  await expect(page.getByText("Narrative Wiki Compiler")).toBeVisible();
  await expect(page.getByText("作者修改") /* no pending fixture */).toHaveCount(0);
  await expect(page.getByText("林澈", { exact: true }).first()).toBeVisible();
  await page.waitForTimeout(350);
  await page.screenshot({ path: path.join("test-results", "wiki.png") });

  await page.getByRole("button", { name: "正文", exact: true }).click();
  await page.waitForTimeout(450);
  await page.screenshot({ path: path.join("test-results", "studio.png") });

  await page.getByRole("button", { name: "设定与配置", exact: true }).click();
  await page.getByRole("button", { name: "文笔", exact: true }).click();
  await page.waitForTimeout(450);
  await page.screenshot({ path: path.join("test-results", "style.png") });

  await page.getByRole("button", { name: "导出", exact: true }).click();
  await page.getByRole("button", { name: "导出 Markdown" }).click();
  await expect(page.getByText(/已导出 10 章/)).toBeVisible();
  await page.waitForTimeout(300);
  await page.screenshot({ path: path.join("test-results", "export.png") });

  await page.getByRole("button", { name: "时间线", exact: true }).click();
  await page.waitForTimeout(500);
  await page.screenshot({ path: path.join("test-results", "map.png") });
  await page.getByRole("button", { name: "关系图", exact: true }).click();
  await page.waitForTimeout(700);
  await page.screenshot({ path: path.join("test-results", "graph.png") });
});

test("agent workbench fits both desktop review sizes", async ({ page }) => {
  await installMock(page);
  await page.goto("/");
  await page.getByRole("button", { name: /^Agent/ }).click();
  await expect(page.getByText("Agent 项目工作台")).toBeVisible();

  await page.setViewportSize({ width: 1440, height: 900 });
  await page.waitForTimeout(200);
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= document.documentElement.clientWidth,
    ),
  ).toBe(true);
  await page.screenshot({ path: path.join("test-results", "agent-1440.png") });

  await page.setViewportSize({ width: 1280, height: 720 });
  await page.waitForTimeout(200);
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= document.documentElement.clientWidth,
    ),
  ).toBe(true);
  await page.screenshot({ path: path.join("test-results", "agent-1280.png") });
});
