// Mirrors src-tauri/src/model/* — see docs/IPC_CONTRACT.md. Keep in sync.

export type HttpMethod = "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD" | "OPTIONS" | "TRACE";

export interface KV {
  name: string;
  value: string;
  enabled?: boolean;
  kind?: string;
}

export type Auth =
  | { type: "none" }
  | { type: "bearer"; token?: string }
  | { type: "basic"; username?: string; password?: string }
  | { type: "apikey"; key?: string; value?: string; in?: "header" | "query" }
  | { type: "digest"; username?: string; password?: string }
  | OAuth2Auth;

export type OAuth2Grant =
  | "clientCredentials"
  | "password"
  | "authorizationCode";

export interface OAuth2Auth {
  type: "oauth2";
  grantType?: OAuth2Grant;
  accessTokenUrl?: string;
  refreshTokenUrl?: string;
  authorizationUrl?: string;
  callbackUrl?: string;
  clientId?: string;
  clientSecret?: string;
  scope?: string;
  username?: string;
  password?: string;
  state?: string;
  pkce?: boolean;
  credentialsPlacement?: "body" | "basicAuthHeader";
  tokenPlacement?: "header" | "query";
  tokenHeaderPrefix?: string;
  tokenQueryKey?: string;
}

export type Body =
  | { type: "none" }
  | { type: "json" | "text" | "xml"; content: string }
  | { type: "form-urlencoded" | "multipart"; items: KV[] }
  | { type: "binary"; path: string }
  | { type: "graphql"; query: string; variables?: string };

export type TestMatcherName =
  | "toBe"
  | "toNotBe"
  | "toEqual"
  | "toContain"
  | "toMatch"
  | "toBeType"
  | "toHaveLength"
  | "toBeGreaterThan"
  | "toBeLessThan"
  | "toBeTruthy"
  | "toBeNull";

export type TestAssertion = { expect: string } & Partial<Record<TestMatcherName, unknown>>;

export type RequestProtocol = "http" | "graphql" | "websocket" | "grpc";

export interface GraphqlSpec {
  operation?: "query" | "mutation" | "subscription";
  endpoint?: string;
}

export interface WebsocketSpec {
  protocols?: string;
}

export interface GrpcSpec {
  service?: string;
  method?: string;
  proto?: string;
}

export type FlowStep = string | { path: string; onFailure?: "stop" | "continue" };

export interface FlowDoc {
  schemaVersion: string;
  name: string;
  kind: "flow";
  steps: FlowStep[];
}

export function flowStepPath(step: FlowStep): string {
  return typeof step === "string" ? step : step.path;
}

export function flowStepStopsOnFailure(step: FlowStep): boolean {
  return typeof step === "string" ? true : step.onFailure !== "continue";
}

export interface FlowTreeNode {
  /** Path relative to `flows/`. */
  path: string;
  name: string;
  kind: "folder" | "flow";
  children?: FlowTreeNode[];
}

export interface RequestDoc {
  schemaVersion: string;
  name: string;
  kind: "request";
  description?: string;
  protocol?: RequestProtocol;
  graphql?: GraphqlSpec;
  websocket?: WebsocketSpec;
  grpc?: GrpcSpec;
  request: {
    method: HttpMethod;
    url: string;
    params?: KV[];
    headers?: KV[];
    pathParams?: KV[];
    body?: Body;
  };
  auth?: Auth;
  variables?: Record<string, string>;
  scripts?: { preRequest?: string; postResponse?: string };
  tests?: TestAssertion[];
}

export interface EnvDoc {
  schemaVersion: string;
  name: string;
  description?: string;
  variables?: Record<string, string>;
  /** Declared secrets: name → default value (committed fallback). The current
   * value lives in the OS keychain and wins when set. */
  secrets?: Record<string, string>;
}

export interface EnvSummary {
  fileName: string;
  name: string;
  description?: string;
  variableCount: number;
  secretCount: number;
}

export interface WorkspaceInfo {
  root: string;
  name: string;
  hasGit: boolean;
  defaultEnvironment?: string | null;
}

export type TreeNodeKind = "folder" | "request" | "collection";

export interface TreeNode {
  path: string;
  name: string;
  kind: TreeNodeKind;
  method?: HttpMethod;
  url?: string;
  children?: TreeNode[];
  meta?: FolderMeta;
}

export interface FolderMeta {
  hasAuth: boolean;
  hasScripts: boolean;
  headerCount: number;
  variableCount: number;
}

export interface TestResult {
  expect: string;
  matcher: string | null;
  expected: unknown;
  actual: unknown | null;
  passed: boolean;
  message: string | null;
}

export interface SendResult {
  requestId: string;
  status: number | null;
  statusText: string;
  ok: boolean;
  timeMs: number;
  sizeBytes: number;
  headers: { name: string; value: string }[];
  cookies: { name: string; value: string }[];
  contentType: string | null;
  bodyText: string | null;
  bodyBase64: string | null;
  truncated: boolean;
  error: string | null;
  variablesUsed: string[];
  missingVariables: string[];
  secretsUsed: string[];
  testResults: TestResult[];
  scriptLogs: string[];
  scriptError: string | null;
  timeline: TimelineEvent[];
  authUsed?: string | null;
}

export interface TimelineEvent {
  ts: string;
  phase: "prepared" | "auth" | "request" | "response" | "redirect" | "error";
  message: string;
}

export type GitEntryStatus =
  | "modified"
  | "added"
  | "deleted"
  | "renamed"
  | "untracked"
  | "conflicted";

export interface GitEntry {
  path: string;
  status: GitEntryStatus;
  staged: boolean;
}

export interface GitStatus {
  hasRepo: boolean;
  branch: string | null;
  entries: GitEntry[];
  remoteUrl?: string | null;
  ahead?: number | null;
  behind?: number | null;
}

export interface GitRemote {
  name: string;
  url: string;
}

export interface GitCommit {
  oid: string;
  shortOid: string;
  message: string;
  author: string;
  time: string;
}

export interface HistoryEntry {
  ts: string;
  method: HttpMethod;
  url: string;
  status: number | null;
  ok: boolean;
  timeMs: number;
  env: string | null;
  requestPath: string | null;
}

export interface HistoryPin {
  ts: string;
  requestPath?: string | null;
}

/** Command ids that can be rebound in Settings. */
export type ShortcutAction =
  | "save"
  | "saveAll"
  | "send"
  | "duplicate"
  | "rename"
  | "closeTab"
  | "closeAllTabs"
  | "nextTab"
  | "prevTab"
  | "newRequest"
  | "codegen"
  | "copyCurl"
  | "palette"
  | "settings"
  | "sidebar"
  | "menu"
  | "console"
  | "splitView"
  | "panelCollections"
  | "panelEnvironments"
  | "panelHistory"
  | "panelGit"
  | "tabParams"
  | "tabHeaders"
  | "tabAuth"
  | "tabBody"
  | "tabScripts"
  | "tabTests"
  | "tabDocs";

export type Shortcuts = Partial<Record<ShortcutAction, string>>;

export interface AppSettings {
  theme: "dark" | "light";
  requestTimeoutSec: number;
  followRedirects: boolean;
  saveOnSend: boolean;
  editorFontSize: number;
  lastWorkspace?: string | null;
  recentWorkspaces?: string[];
  proxyUrl?: string | null;
  insecureTls?: boolean;
  caCertPath?: string | null;
  sendCookies?: boolean;
  storeCookies?: boolean;
  maxRedirects?: number;
  /** Debounced save of dirty tabs. Off by default. */
  autoSave?: boolean;
  /** Milliseconds to wait after the last edit before auto-saving. */
  autoSaveInterval?: number;
  /** User overrides of the default keyboard shortcuts. */
  shortcuts?: Shortcuts;
  /** `off` | `openai` | `anthropic` | `custom`. The API key is not stored here. */
  aiProvider?: "off" | "openai" | "anthropic" | "custom";
  aiModel?: string;
  /** OpenAI-compatible base URL. Empty uses the provider default. */
  aiBaseUrl?: string | null;
}

// ---------- v1.1 types ----------

export interface FolderDoc {
  schemaVersion: string;
  kind?: string;
  name?: string;
  description?: string;
  variables?: Record<string, string>;
  auth?: Auth;
  headers?: KV[];
  scripts?: Scripts;
}

export interface Scripts {
  preRequest?: string;
  postResponse?: string;
}

export interface CollectionDoc {
  schemaVersion: string;
  name: string;
  description?: string;
  variables?: Record<string, string>;
  defaultEnvironment?: string;
  auth?: Auth;
  headers?: KV[];
  scripts?: Scripts;
}

export interface RunOptions {
  delayMs?: number;
  stopOnFailure?: boolean;
  recursive?: boolean;
  dataFile?: string | null;
}

export interface RunnerItem {
  path: string;
  name: string;
  method: string;
  status: "running" | "passed" | "failed" | "error" | "skipped";
  statusCode?: number;
  timeMs: number;
  sizeBytes: number;
  testsTotal: number;
  testsPassed: number;
  iteration?: number;
  error?: string;
}

export interface RunnerSummary {
  total: number;
  passed: number;
  failed: number;
  errored: number;
  skipped: number;
  durationMs: number;
}

export interface RunnerEvent {
  runId: string;
  kind: "item" | "done";
  item?: RunnerItem;
  summary?: RunnerSummary;
}

export interface GqlTypeRef {
  kind: string;
  name?: string;
  ofType?: GqlTypeRef;
}

export interface GqlField {
  name: string;
  description?: string;
  args: { name: string; description?: string; defaultValue?: string; type: GqlTypeRef }[];
  type: GqlTypeRef;
}

export interface GqlType {
  kind: string;
  name: string;
  description?: string;
  fields: GqlField[];
  enumValues: { name: string; description?: string }[];
}

export interface GqlSchema {
  queryType?: string;
  mutationType?: string;
  subscriptionType?: string;
  types: GqlType[];
}

export interface WsEvent {
  sessionId: string;
  kind: "open" | "message" | "close" | "error";
  data?: string;
  opcode?: string;
}

export interface GrpcEvent {
  sessionId: string;
  kind: "message" | "trailers" | "error" | "end";
  bodyText?: string | null;
  grpcStatus?: number | null;
  grpcMessage?: string | null;
}

export interface ProtoField {
  name: string;
  number: number;
  typeName: string;
  repeated?: boolean;
}

export interface ProtoFile {
  package: string;
  services: {
    name: string;
    package: string;
    methods: {
      name: string;
      input: string;
      output: string;
      clientStreaming?: boolean;
      serverStreaming?: boolean;
    }[];
  }[];
  messages: { name: string; fields: ProtoField[] }[];
}

export interface GrpcResult {
  status: number;
  grpcStatus: number;
  grpcMessage: string;
  timeMs: number;
  bodyText: string;
  headers: [string, string][];
}

export interface CookieDto {
  name: string;
  value: string;
  domain: string;
  path: string;
  secure: boolean;
  httpOnly: boolean;
  expires?: string;
}

export interface ImportResult {
  files: string[];
  skipped: number;
  warnings: string[];
}

export const HTTP_METHODS: HttpMethod[] = [
  "GET",
  "POST",
  "PUT",
  "PATCH",
  "DELETE",
  "HEAD",
  "OPTIONS",
  "TRACE",
];

export function emptyRequestDoc(name: string): RequestDoc {
  return {
    schemaVersion: "1",
    name,
    kind: "request",
    request: { method: "GET", url: "{{baseUrl}}/" },
  };
}

export function emptyEnvDoc(name: string): EnvDoc {
  return {
    schemaVersion: "1",
    name,
    variables: {},
    secrets: {},
  };
}
