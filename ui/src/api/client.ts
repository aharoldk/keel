import { invoke } from "@tauri-apps/api/core";
import type {
  AppSettings,
  CollectionDoc,
  CookieDto,
  EnvDoc,
  EnvSummary,
  FolderDoc,
  GitCommit,
  GitRemote,
  GitStatus,
  GqlSchema,
  GrpcResult,
  HistoryEntry,
  HistoryPin,
  ImportResult,
  KV,
  ProtoFile,
  FlowDoc,
  RequestDoc,
  RunOptions,
  RunnerEvent,
  SendResult,
  TreeNode,
  WorkspaceInfo,
} from "./types";

export const api = {
  // workspace & collection
  workspaceOpen: (path: string) =>
    invoke<WorkspaceInfo>("workspace_open", { path }),
  workspaceInit: (path: string, name: string) =>
    invoke<WorkspaceInfo>("workspace_init", { path, name }),
  workspaceClose: () => invoke<void>("workspace_close"),
  workspaceInfo: () => invoke<WorkspaceInfo | null>("workspace_info"),
  loadTree: () => invoke<TreeNode[]>("workspace_load_tree"),
  folderCreate: (parent: string, name: string) =>
    invoke<string>("folder_create", { parent, name }),
  requestCreate: (folder: string, name: string) =>
    invoke<string>("request_create", { folder, name }),
  requestRead: (path: string) => invoke<RequestDoc>("request_read", { path }),
  requestSave: (path: string, doc: RequestDoc) =>
    invoke<void>("request_save", { path, doc }),
  requestRename: (path: string, newName: string) =>
    invoke<string>("request_rename", { path, newName }),
  requestDuplicate: (path: string) => invoke<string>("request_duplicate", { path }),
  nodeDelete: (path: string) => invoke<void>("node_delete", { path }),
  nodeMove: (path: string, dest: string) =>
    invoke<string>("node_move", { path, dest }),
  nodeReorder: (path: string, target: string, before: boolean) =>
    invoke<string>("node_reorder", { path, target, before }),

  // environments & secrets
  envList: () => invoke<EnvSummary[]>("env_list"),
  envRead: (fileName: string) => invoke<EnvDoc>("env_read", { fileName }),
  envSave: (fileName: string | null, doc: EnvDoc) =>
    invoke<string>("env_save", { fileName, doc }),
  envDelete: (fileName: string) => invoke<void>("env_delete", { fileName }),
  // Current values: local variable overrides (never committed). An empty
  // value clears the override so the default value is used again.
  envValuesRead: (fileName: string) =>
    invoke<Record<string, string>>("env_values_read", { fileName }),
  envValueSet: (fileName: string, name: string, value: string) =>
    invoke<void>("env_value_set", { fileName, name, value }),
  envValueDelete: (fileName: string, name: string) =>
    invoke<void>("env_value_delete", { fileName, name }),
  secretSet: (envName: string, name: string, value: string) =>
    invoke<void>("secret_set", { envName, name, value }),
  secretDelete: (envName: string, name: string) =>
    invoke<void>("secret_delete", { envName, name }),
  secretList: (envName: string) => invoke<string[]>("secret_list", { envName }),

  // http
  sendRequest: (
    path: string,
    envName: string | null,
    doc?: RequestDoc | null,
  ) => invoke<SendResult>("send_request", { path, envName, doc }),

  // history
  historyList: (limit?: number) =>
    invoke<HistoryEntry[]>("history_list", { limit }),
  historyClear: () => invoke<void>("history_clear"),
  historyPins: () => invoke<HistoryPin[]>("history_pins"),
  historyPin: (pin: HistoryPin) => invoke<void>("history_pin", { pin }),
  historyUnpin: (ts: string, requestPath: string | null) =>
    invoke<void>("history_unpin", { ts, requestPath }),

  // git
  gitStatus: () => invoke<GitStatus>("git_status"),
  gitStage: (paths: string[] | null) => invoke<void>("git_stage", { paths }),
  gitUnstage: (paths: string[] | null) => invoke<void>("git_unstage", { paths }),
  gitCommit: (message: string) => invoke<string>("git_commit", { message }),
  gitLog: (limit?: number) => invoke<GitCommit[]>("git_log", { limit }),
  gitInit: () => invoke<void>("git_init"),
  gitDiffFile: (path: string) => invoke<string>("git_diff_file", { path }),
  gitBranches: () => invoke<string[]>("git_branches"),
  gitCheckout: (name: string) => invoke<void>("git_checkout", { name }),
  gitCreateBranch: (name: string) => invoke<void>("git_create_branch", { name }),
  gitRemotes: () => invoke<GitRemote[]>("git_remotes"),
  gitSetRemote: (url: string) => invoke<void>("git_set_remote", { url }),
  gitPull: () => invoke<string>("git_pull"),
  gitPush: () => invoke<string>("git_push"),
  gitResolve: (path: string, side: "ours" | "theirs") =>
    invoke<void>("git_resolve", { path, side }),

  // import / export
  importCurl: (text: string, folder: string) =>
    invoke<string>("import_curl", { text, folder }),
  importOpenapi: (sourcePath: string, folder: string) =>
    invoke<{ files: string[]; skipped: number; warnings: string[] }>(
      "import_openapi",
      { sourcePath, folder },
    ),
  importSource: (text: string, folder: string) =>
    invoke<ImportResult>("import_source", { text, folder }),
  importZip: (path: string | null, dataBase64: string | null, folder: string) =>
    invoke<ImportResult>("import_zip", { path, dataBase64, folder }),
  importGit: (url: string, parent: string | null) =>
    invoke<string>("import_git", { url, parent }),
  readTextFile: (path: string) => invoke<string>("read_text_file", { path }),
  fetchUrl: (url: string) => invoke<string>("fetch_url", { url }),
  exportCurl: (path: string, doc?: RequestDoc | null) =>
    invoke<string>("export_curl", { path, doc }),
  saveResponse: (path: string, dataBase64: string) =>
    invoke<void>("save_response", { path, dataBase64 }),

  // v1.1: collection & folder metadata
  collectionRead: () => invoke<CollectionDoc>("collection_read"),
  collectionSave: (doc: CollectionDoc) => invoke<void>("collection_save", { doc }),
  folderRead: (path: string) => invoke<FolderDoc | null>("folder_read", { path }),
  folderSave: (path: string, doc: FolderDoc) =>
    invoke<void>("folder_save", { path, doc }),
  folderDeleteMeta: (path: string) => invoke<void>("folder_delete_meta", { path }),

  // runner
  runFolder: (path: string, envName: string | null, options: RunOptions) =>
    invoke<string>("run_folder", { path, envName, options }),
  runCancel: (runId: string) => invoke<void>("run_cancel", { runId }),

  // cookies
  cookieList: () => invoke<CookieDto[]>("cookie_list"),
  cookieDelete: (domain: string, name: string) =>
    invoke<void>("cookie_delete", { domain, name }),
  cookieClear: () => invoke<void>("cookie_clear"),

  // import / export / codegen
  importPostman: (sourcePath: string, folder: string) =>
    invoke<ImportResult>("import_postman", { sourcePath, folder }),
  exportOpenapi: (folder: string) => invoke<string>("export_openapi", { folder }),
  generateCode: (path: string, target: string, doc?: RequestDoc | null) =>
    invoke<string>("generate_code", { path, target, doc }),

  // app
  settingsGet: () => invoke<AppSettings>("settings_get"),
  settingsSet: (settings: AppSettings) =>
    invoke<void>("settings_set", { settings }),
  appVersion: () => invoke<string>("get_app_version"),

  graphqlIntrospect: (url: string, headers: KV[]) =>
    invoke<GqlSchema>("graphql_introspect", { args: { url, headers } }),
  graphqlBuildQuery: (
    operation: string,
    rootField: string,
    selection: unknown[],
  ) =>
    invoke<string>("graphql_build_query", {
      args: { operation, rootField, selection },
    }),

  wsConnect: (sessionId: string, url: string, headers: KV[], protocols: string[]) =>
    invoke<void>("ws_connect", { args: { sessionId, url, headers, protocols } }),
  wsSend: (sessionId: string, data: string, binary = false) =>
    invoke<void>("ws_send", { args: { sessionId, data, binary } }),
  wsClose: (sessionId: string) => invoke<void>("ws_close", { sessionId }),

  grpcParseProto: (proto: string) =>
    invoke<ProtoFile>("grpc_parse_proto", { args: { proto } }),
  grpcCall: (args: {
    url: string;
    service: string;
    method: string;
    body: unknown;
    fields: ProtoFile["messages"][number]["fields"];
    messages: ProtoFile["messages"];
    headers: KV[];
  }) => invoke<GrpcResult>("grpc_call", { args }),

  grpcOpen: (args: {
    sessionId: string;
    url: string;
    service: string;
    method: string;
    body: unknown;
    fields: ProtoFile["messages"][number]["fields"];
    messages: ProtoFile["messages"];
    headers: KV[];
  }) => invoke<void>("grpc_open", { args }),
  grpcClose: (sessionId: string) => invoke<void>("grpc_close", { sessionId }),

  aiStatus: () => invoke<{ configured: boolean }>("ai_status"),
  aiKeySet: (key: string) => invoke<void>("ai_key_set", { key }),
  aiKeyClear: () => invoke<void>("ai_key_clear"),
  aiGenerate: (
    kind: "script" | "test" | "docs" | "request",
    prompt: string,
    context: string,
  ) => invoke<string>("ai_generate", { args: { kind, prompt, context } }),
  flowList: () => invoke<{ fileName: string; name: string }[]>("flow_list"),
  flowRead: (fileName: string) => invoke<FlowDoc>("flow_read", { fileName }),
  flowSave: (fileName: string | null, doc: FlowDoc) =>
    invoke<string>("flow_save", { fileName, doc }),
  flowDelete: (fileName: string) => invoke<void>("flow_delete", { fileName }),
  flowImport: (path: string) => invoke<string>("flow_import", { path }),
  flowToYaml: (doc: FlowDoc) => invoke<string>("flow_to_yaml", { doc }),
  exportRequest: (path: string) => invoke<string>("export_request", { path }),
  exportCollection: (folder: string) => invoke<string>("export_collection", { folder }),
  requestToYaml: (doc: RequestDoc) => invoke<string>("request_to_yaml", { doc }),
  requestFromYaml: (yaml: string) => invoke<RequestDoc>("request_from_yaml", { yaml }),
};
