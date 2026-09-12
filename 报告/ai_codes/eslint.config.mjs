import { createRequire } from "node:module";
const require = createRequire(new URL("../../mod_UI/package.json", import.meta.url));
const ts = require("typescript-eslint");
const hooks = require("eslint-plugin-react-hooks");
export default [{
  files: ["src/**/*.{ts,tsx}"],
  ignores: ["src/**/*.test.*"],
  languageOptions: { parser: ts.parser, parserOptions: { ecmaFeatures: { jsx: true } } },
  plugins: { "react-hooks": hooks },
  rules: { "react-hooks/rules-of-hooks": "error", "react-hooks/exhaustive-deps": "error" },
}];
