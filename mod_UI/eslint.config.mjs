import { createRequire } from "node:module";

const require = createRequire(new URL("./package.json", import.meta.url));
const ts = require("typescript-eslint");
const hooks = require("eslint-plugin-react-hooks");

const maxLen = [
  "warn",
  {
    code: 200,
    ignoreUrls: true,
    ignoreStrings: true,
    ignoreTemplateLiterals: true,
  },
];

export default [
  {
    ignores: [
      "dist/**",
      "node_modules/**",
      "src-tauri/**",
      "src/**/*.test.ts",
      "src/**/*.test.tsx",
    ],
  },
  ...ts.configs.recommended,
  {
    files: ["src/**/*.{ts,tsx}"],
    ignores: ["src/**/*.test.*"],
    languageOptions: {
      parser: ts.parser,
      parserOptions: { ecmaFeatures: { jsx: true } },
    },
    plugins: { "react-hooks": hooks },
    rules: {
      "react-hooks/rules-of-hooks": "error",
      "react-hooks/exhaustive-deps": "error",
      // 防止再次出现把整段逻辑压成一行的写法。
      // 既有源码仍有约 208 处超长行，先以 warn 落地；本次新增模块保持 error。
      "max-len": maxLen,
    },
  },
  {
    // 本次新增模块必须保持严格行宽。
    files: ["src/utils-url.ts", "src/useScriptGeneration.ts"],
    rules: {
      "max-len": [
        "error",
        {
          code: 200,
          ignoreUrls: true,
          ignoreStrings: true,
          ignoreTemplateLiterals: true,
        },
      ],
    },
  },
];
