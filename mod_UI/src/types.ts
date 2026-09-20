import type { SearchResult } from "./utils";

export type View = "workspace" | "report" | "history" | "settings";
export type Ecosystem = "r" | "r-binary" | "pip" | "conda";
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
  rLibPath: string;
  fullSearch: boolean;
  searchConcurrency: number;
  archiveGithubMajorGap: number;
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
  pinnedMethods: Method[];
  pipIndex: string;
  condaChannels: string[];
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
  rLibPath: "",
  fullSearch: false,
  searchConcurrency: 6,
  archiveGithubMajorGap: 1,
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
  pinnedMethods: ["auto", "base", "biocManager", "github"],
  pipIndex: "https://pypi.org",
  condaChannels: ["conda-forge", "bioconda"],
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

export const defaultPinnedMethods: Method[] = [...defaultSettings.pinnedMethods];

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
  { label: "Posit Package Manager", value: "https://packagemanager.posit.co/cran/latest" },
  { label: "清华大学", value: "https://mirrors.tuna.tsinghua.edu.cn/CRAN/" },
  { label: "中国科学技术大学", value: "https://mirrors.ustc.edu.cn/CRAN/" },
  { label: "北京外国语大学", value: "https://mirrors.bfsu.edu.cn/CRAN/" },
];

export const sourceNames: Record<string, string> = {
  cran: "CRAN",
  "cran-binary": "R 二进制镜像",
  bioc: "Bioconductor",
  biocGit: "Bioc 历史版",
  github: "GitHub",
  "r-forge": "R-Forge",
  none: "未找到",
  pip: "Pip",
  conda: "Conda",
};

/** 应用更新的状态机；界面（侧边栏版本徽标、设置页）与更新流程共用同一份定义。 */
export type UpdateState =
  | "idle"
  | "checking"
  | "available"
  | "downloading"
  | "installing"
  | "readyToRestart"
  | "upToDate"
  | "error";

/** 更新失败的阶段：用于把「跑不通」拆成可执行的结论，而不是一句笼统的失败。 */
export type UpdateFailureStage =
  | "manifest-missing"
  | "signature"
  | "permission"
  | "network"
  | "unknown";

/** 由后端 `inspect_updater_config` 读取的、真正生效的更新配置。 */
export interface UpdaterConfigInfo {
  endpoints: string[];
  pubkeyKeyId: string | null;
  currentVersion: string;
}
