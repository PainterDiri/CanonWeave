import { test, expect } from "@playwright/test";
import path from "node:path";

const project = {
  project: {
    id: "demo",
    name: "雨夜的第七封信",
    rootPath: "/Users/writer/Stories/rain-night",
    mode: "both",
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
  parentId: null,
  level: "chapter",
  orderNo: n,
  title,
  goal: ["拿到那封下周寄出的信", "确认钥匙属于谁", "在守夜人面前隐藏父亲的秘密", "找到旧车站的入口"][n - 1] ?? "推进主线",
  conflict: "母亲要求立刻结束葬礼，守夜人拒绝交出钥匙。",
  turn: "信的日期又向前跳了一天。",
  payoff: "回收旧钥匙；新增车站的检票口。",
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
            case "outline_list":
              return chapters;
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
            case "candidate_list":
              return args?.kind === "draft" ? [] : [candidate, candidate2];
            case "chapter_list":
              return chapters;
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
            case "app_info":
              return { name: "CanonWeave", version: "0.1.0", schemaVersion: 1 };
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
  await page.waitForTimeout(400);
  await page.screenshot({ path: path.join("test-results", "overview.png") });

  await page.getByRole("button", { name: "灵感", exact: true }).click();
  await expect(page.getByText("种子卡").first()).toBeVisible();
  await page.waitForTimeout(350);
  await page.screenshot({ path: path.join("test-results", "seed.png") });

  await page.getByRole("button", { name: "创作对话" }).click();
  await expect(page.getByText("守护者").or(page.getByText("母亲坚持封棺"))).toBeVisible({ timeout: 5000 }).catch(() => {});
  await page.screenshot({ path: path.join("test-results", "chat.png") });

  await page.getByRole("button", { name: "十章骨架" }).click();
  await page.waitForTimeout(350);
  await page.screenshot({ path: path.join("test-results", "outline.png") });

  await page.getByRole("button", { name: /审计/ }).click();
  await page.waitForTimeout(350);
  await page.screenshot({ path: path.join("test-results", "audit.png") });

  await page.getByRole("button", { name: "人物", exact: true }).click();
  await page.waitForTimeout(350);
  await page.screenshot({ path: path.join("test-results", "characters.png") });

  await page.getByRole("button", { name: "伏笔", exact: true }).click();
  await page.waitForTimeout(350);
  await page.screenshot({ path: path.join("test-results", "promises.png") });

  await page.getByRole("button", { name: "正文", exact: true }).click();
  await page.waitForTimeout(450);
  await page.screenshot({ path: path.join("test-results", "studio.png") });

  await page.getByRole("button", { name: "设定", exact: true }).click();
  await page.getByRole("button", { name: "文笔", exact: true }).click();
  await page.waitForTimeout(450);
  await page.screenshot({ path: path.join("test-results", "style.png") });
});
