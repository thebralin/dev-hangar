import { invoke } from "@tauri-apps/api/core";

export interface Stack { id: string; label: string }
export interface Action { id: string; label: string; command: string; kind: string; cwd: string }
export interface Dependency { name: string; version: string; dev: boolean; source: string; scope: string }
export interface Module {
  relPath: string;
  name: string;
  version?: string | null;
  description?: string | null;
  stacks: Stack[];
  packageManager?: string | null;
  scripts: [string, string][];
  actions: Action[];
  needsInstall: boolean;
  nodeVersion?: string | null;
  phpVersion?: string | null;
  vendorMissing: boolean;
  composerScripts: [string, string][];
  missingTools: string[];
}
export interface ProjectInfo {
  name: string;
  path: string;
  exists: boolean;
  root: Module;
  subprojects: Module[];
  stacks: Stack[];
  dependencies: Dependency[];
  envFiles: string[];
  readme?: string | null;
  notableFiles: string[];
  isGit: boolean;
  modified?: number | null;
}
export interface RecentFile { path: string; modified: number; size: number }
export interface ProjectStats {
  files: number;
  dirs: number;
  totalSize: number;
  lastModified?: number | null;
  recent: RecentFile[];
  languages: [string, number][];
  truncated: boolean;
}
export interface Commit {
  hash: string; short: string; author: string; email: string; date: string;
  subject: string; refs: string; filesChanged: number; insertions: number; deletions: number;
}
export interface Remote { name: string; url: string; webUrl?: string | null }
export interface StatusEntry { path: string; index: string; worktree: string }
export interface GitSummary {
  isRepo: boolean; gitAvailable: boolean; root?: string | null; branch?: string | null;
  upstream?: string | null; ahead: number; behind: number; remotes: Remote[]; branches: string[];
  tags: string[]; status: StatusEntry[]; lastCommit?: Commit | null; firstCommitDate?: string | null;
  commitCount: number; stashCount: number; userName?: string | null; userEmail?: string | null;
  operation?: "merge" | "rebase" | "cherry-pick" | "revert" | null; remoteBranches: string[]; stashes: Stash[];
}
export interface Stash { index: number; message: string; date: string }
export type GitOp =
  | { op: "stage" | "unstage" | "discard"; paths: string[] }
  | { op: "commit"; message: string; amend: boolean; all: boolean }
  | { op: "fetch" | "abort" | "continue" | "pushTags" | "undoCommit" }
  | { op: "pull"; rebase: boolean }
  | { op: "push"; force: boolean }
  | { op: "checkout"; branch: string }
  | { op: "createBranch"; name: string; from: string | null; checkout: boolean }
  | { op: "deleteBranch"; name: string; force: boolean }
  | { op: "renameBranch"; from: string; to: string }
  | { op: "merge"; branch: string; noFf: boolean }
  | { op: "rebase"; onto: string }
  | { op: "resolve"; path: string; side: "ours" | "theirs" }
  | { op: "stashPush"; message: string | null; includeUntracked: boolean }
  | { op: "stashApply"; index: number; pop: boolean }
  | { op: "stashDrop"; index: number }
  | { op: "createTag"; name: string; at: string | null; message: string | null }
  | { op: "deleteTag"; name: string }
  | { op: "cherryPick" | "revert"; hash: string }
  | { op: "reset"; hash: string; mode: "soft" | "mixed" | "hard" }
  | { op: "setUser"; name: string; email: string };
export interface ChangedFile { status: string; path: string; oldPath?: string | null; additions?: number | null; deletions?: number | null }
export interface CommitDetail { hash: string; author: string; email: string; date: string; message: string; parents: string[]; files: ChangedFile[] }
export interface DirEntry { name: string; path: string; isDir: boolean; size: number; modified: number; heavy: boolean; hidden: boolean }
export interface FileContent { path: string; content: string; size: number; modified: number; binary: boolean; truncated: boolean }
export interface FileHit { path: string; rel: string; score: number; isDir: boolean }
export interface TextHit { path: string; rel: string; line: number; text: string }

export interface Checkpoint { id: string; hash: string; date: string; message: string; branch: string }
export interface CheckpointDiff { files: [string, string][]; patch: string }
export interface TodoHit { path: string; line: number; kind: string; text: string }
export interface ArtifactDir { path: string; rel: string; name: string; kind: string; size: number; files: number; modified: number }
export interface DeleteResult { path: string; ok: boolean; error?: string | null }

export interface Day { date: string; commits: number; adds: number; dels: number }
export interface Author { name: string; email: string; commits: number; adds: number; dels: number; first: string; last: string }
export interface HotFile { path: string; commits: number; adds: number; dels: number; last: string }
export interface GitAnalytics {
  isRepo: boolean; totalCommits: number; totalAdds: number; totalDels: number;
  firstDate?: string | null; lastDate?: string | null; days: Day[]; punch: number[][];
  authors: Author[]; hotFiles: HotFile[]; truncated: boolean;
}
export interface LangStat { lang: string; files: number; lines: number; code: number; bytes: number }
export interface BigFile { path: string; rel: string; lines: number; lang: string }
export interface CodeStats { languages: LangStat[]; totalFiles: number; totalLines: number; totalCode: number; biggest: BigFile[]; truncated: boolean }

export interface GraphNode { id: string; path: string; lang: string; lines: number; external: boolean }
export interface GraphEdge { source: string; target: string; kind: "import" | "ipc"; count: number }
export interface CodeGraph { nodes: GraphNode[]; edges: GraphEdge[]; truncated: boolean }

export interface PluginEntry { folder: string; dir: string; manifest?: Record<string, unknown> | null; error?: string | null }
export interface ExecResult { code: number | null; stdout: string; stderr: string; timedOut: boolean }

export const api = {
  pluginsDir: () => invoke<string>("plugins_dir"),
  pluginList: () => invoke<PluginEntry[]>("plugin_list"),
  pluginReadFile: (folder: string, rel: string) => invoke<string>("plugin_read_file", { folder, rel }),
  pluginFilePath: (folder: string, rel: string) => invoke<string>("plugin_file_path", { folder, rel }),
  pluginInstall: (src: string) => invoke<string>("plugin_install", { src }),
  pluginUninstall: (folder: string) => invoke<void>("plugin_uninstall", { folder }),
  pluginScaffold: (id: string, name: string) => invoke<string>("plugin_scaffold", { id, name }),
  execCapture: (cwd: string, command: string, timeoutMs?: number) => invoke<ExecResult>("exec_capture", { cwd, command, timeoutMs }),
  writeTextFile: (path: string, content: string) => invoke<void>("write_text_file", { path, content }),
  codeGraph: (root: string) => invoke<CodeGraph>("code_graph", { root }),
  gitAnalytics: (path: string, maxCommits?: number) => invoke<GitAnalytics>("git_analytics", { path, maxCommits }),
  codeStats: (root: string) => invoke<CodeStats>("code_stats", { root }),
  checkpointCreate: (path: string, message: string) => invoke<Checkpoint>("checkpoint_create", { path, message }),
  checkpointList: (path: string) => invoke<Checkpoint[]>("checkpoint_list", { path }),
  checkpointDiff: (path: string, hash: string, file?: string) => invoke<CheckpointDiff>("checkpoint_diff", { path, hash, file }),
  checkpointRestore: (path: string, hash: string) => invoke<Checkpoint>("checkpoint_restore", { path, hash }),
  checkpointDelete: (path: string, id: string) => invoke<void>("checkpoint_delete", { path, id }),
  gitInit: (path: string) => invoke<void>("git_init", { path }),
  scanTodos: (paths: string[], limit?: number) => invoke<TodoHit[]>("scan_todos", { paths, limit }),
  fileTree: (root: string, maxDepth?: number, maxEntries?: number) => invoke<string>("file_tree", { root, maxDepth, maxEntries }),
  writeAiFile: (root: string, name: string, content: string, overwrite: boolean) => invoke<string>("write_ai_file", { root, name, content, overwrite }),
  scanArtifacts: (root: string) => invoke<ArtifactDir[]>("scan_artifacts", { root }),
  deleteArtifacts: (roots: string[], paths: string[]) => invoke<DeleteResult[]>("delete_artifacts", { roots, paths }),
  analyze: (path: string) => invoke<ProjectInfo>("analyze_project", { path }),
  stats: (path: string) => invoke<ProjectStats>("project_stats", { path }),
  gitSummary: (path: string) => invoke<GitSummary>("git_summary", { path }),
  gitLog: (path: string, skip = 0, limit = 50, file?: string) => invoke<Commit[]>("git_log", { path, skip, limit, file }),
  gitCommit: (path: string, hash: string) => invoke<CommitDetail>("git_commit", { path, hash }),
  gitFileDiff: (path: string, file: string, hash?: string) => invoke<string>("git_file_diff", { path, hash, file }),
  gitWorkingDiff: (path: string) => invoke<string>("git_working_diff", { path }),
  gitFileAt: (path: string, hash: string, file: string) => invoke<string>("git_file_at", { path, hash, file }),
  gitOp: (path: string, op: GitOp) => invoke<string>("git_op", { path, op }),
  listDir: (path: string) => invoke<DirEntry[]>("list_dir", { path }),
  readText: (path: string) => invoke<FileContent>("read_text_file", { path }),
  searchFiles: (root: string, query: string, limit?: number) => invoke<FileHit[]>("search_files", { root, query, limit }),
  searchText: (root: string, query: string, limit?: number) => invoke<TextHit[]>("search_text", { root, query, limit }),
  pathExists: (path: string) => invoke<boolean>("path_exists", { path }),
  isDir: (path: string) => invoke<boolean>("is_dir", { path }),
  run: (id: string, cwd: string, command: string, env?: Record<string, string>) => invoke<number>("run_command", { id, cwd, command, env }),
  kill: (id: string) => invoke<void>("kill_command", { id }),
  running: () => invoke<string[]>("running_commands"),
  writeStdin: (id: string, data: string) => invoke<void>("write_stdin", { id, data }),
  coverCapture: (project: string, url: string, width?: number, height?: number) => invoke<string>("cover_capture", { project, url, width, height }),
  coverImport: (project: string, src: string) => invoke<string>("cover_import", { project, src }),
  coverRemove: (project: string) => invoke<void>("cover_remove", { project }),
  openInEditor: (editor: string, path: string, line?: number) => invoke<void>("open_in_editor", { editor, path, line }),
  openTerminal: (path: string, terminal?: string) => invoke<void>("open_terminal", { path, terminal }),
  detectEditors: () => invoke<string[]>("detect_editors"),
  loadConfig: () => invoke<any>("load_config"),
  saveConfig: (config: any) => invoke<void>("save_config", { config }),
  configLocation: () => invoke<string>("config_location"),
};

export function sepOf(p: string) {
  return p.includes("\\") ? "\\" : "/";
}

export function joinPath(base: string, ...parts: string[]) {
  const sep = sepOf(base);
  let out = base.replace(/[\\/]+$/, "");
  for (const part of parts) {
    if (!part) continue;
    out += sep + part.replace(/[\\/]+/g, sep).replace(/^[\\/]+/, "");
  }
  return out;
}

export function baseName(p: string) {
  return p.replace(/[\\/]+$/, "").split(/[\\/]/).pop() || p;
}

export function parentDir(p: string) {
  const parts = p.replace(/[\\/]+$/, "").split(/[\\/]/);
  parts.pop();
  return parts.join(sepOf(p));
}

export function relPath(root: string, p: string) {
  const r = root.replace(/\\/g, "/").replace(/\/+$/, "");
  const q = p.replace(/\\/g, "/");
  if (q.toLowerCase().startsWith(r.toLowerCase() + "/")) return q.slice(r.length + 1);
  if (q.toLowerCase() === r.toLowerCase()) return "";
  return q;
}

/** Convert slashes to match the style of `like` (Windows paths keep backslashes). */
export function sameSep(p: string, like: string) {
  return sepOf(like) === "\\" ? p.replace(/\//g, "\\") : p.replace(/\\/g, "/");
}
