import type { SearchResult } from "./utils";

export type View = "workspace" | "report" | "history" | "settings";
export type Method =
  | "auto"
  | "devtools"
  | "remotes"
  | "github"
  | "base"
  | "version"
  | "biocManager"
  | "checkSystem";

export interface Settings {
  proxy: string;
  githubToken: string;
  cranMirror: string;
  fullSearch: boolean;
  conditional: boolean;
  installDependencies: boolean;
  showRemoteVersion: boolean;
  useCache: boolean;
  maxCacheEntries: number;
  useFilter: boolean;
  resolveDependencies: boolean;
  maxDependencyDepth: number;
  includeLightDependencies: boolean;
  maxDependencyNodes: number;
}

export interface InputRules {
  separators: string[];
  stripQuotes: boolean;
  stripCParens: boolean;
  commentChars: string[];
  splitSpaces: boolean;
  excludeRegex: string[];
  excludeKeywords: string[];
}

export interface InputProfile {
  total: number;
  archiveUrls: number;
  repositories: number;
}

export interface SearchLogBatchEvent {
  runId: number;
  messages: string[];
}

export interface SearchProgressEvent {
  runId: number;
  result: SearchResult;
}

export const defaultSettings: Settings = {
  proxy: "",
  githubToken: "",
  cranMirror: "https://cloud.r-project.org",
  fullSearch: false,
  conditional: true,
  installDependencies: true,
  showRemoteVersion: true,
  useCache: true,
  maxCacheEntries: 1000,
  useFilter: true,
  resolveDependencies: true,
  maxDependencyDepth: 2,
  includeLightDependencies: false,
  maxDependencyNodes: 100,
};

export const defaultInputRules: InputRules = {
  separators: [",", ";"],
  stripQuotes: true,
  stripCParens: true,
  commentChars: ["#"],
  splitSpaces: false,
  excludeRegex: [],
  excludeKeywords: [],
};

export const defaultPinnedMethods: Method[] = ["auto", "base", "biocManager", "github"];

export const methods: Array<{
  id: Method;
  title: string;
  description: string;
}> = [
  { id: "auto", title: "智能路由", description: "根据检索结果自动选择来源" },
  { id: "base", title: "CRAN", description: "install.packages" },
  { id: "biocManager", title: "Bioconductor", description: "BiocManager::install" },
  { id: "github", title: "GitHub", description: "remotes::install_github" },
  { id: "remotes", title: "远程地址", description: "remotes::install_url" },
  { id: "devtools", title: "devtools", description: "devtools::install_url" },
  { id: "version", title: "版本查询", description: "packageVersion" },
  { id: "checkSystem", title: "包加载检测", description: "批量检测安装与加载报错" },
];

export const mirrors = [
  { label: "Posit Cloud", value: "https://cloud.r-project.org" },
  { label: "清华大学", value: "https://mirrors.tuna.tsinghua.edu.cn/CRAN/" },
  { label: "中国科学技术大学", value: "https://mirrors.ustc.edu.cn/CRAN/" },
  { label: "北京外国语大学", value: "https://mirrors.bfsu.edu.cn/CRAN/" },
];

export const sourceNames: Record<string, string> = {
  cran: "CRAN",
  bioc: "Bioconductor",
  biocGit: "Bioc 历史版",
  github: "GitHub",
  none: "未找到",
};
