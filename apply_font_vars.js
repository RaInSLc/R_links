const fs = require('fs');
const path = require('path');
const cssPath = path.join(__dirname, 'mod_UI/src/App.css');
let css = fs.readFileSync(cssPath, 'utf8');

// Step 1: Add CSS variables to :root definition
const rootDefinitionRegex = /:root, \[data-theme="office"\] \{([\s\S]*?)--ink:/;
const fontVars = `
  /* Font Variables */
  --font-base: 'Inter', 'Noto Sans SC', sans-serif;
  --font-mono: 'DM Mono', Consolas, 'Noto Sans SC', monospace;
  
  --ink:`;
css = css.replace(rootDefinitionRegex, `:root, [data-theme="office"] {$1${fontVars}`);

// Add font definitions at the end of the file for the root element based on data-font attribute
const globalFontStyles = `
/* Global Font Settings */
:root[data-font="modern"], [data-theme="office"][data-font="modern"] {
  --font-base: 'Inter', 'Noto Sans SC', sans-serif;
  --font-mono: 'DM Mono', Consolas, 'Noto Sans SC', monospace;
}

:root[data-font="system"], [data-theme="office"][data-font="system"] {
  --font-base: "Segoe UI", "Microsoft YaHei UI", sans-serif;
  --font-mono: Consolas, "Courier New", monospace;
}

:root[data-font="classic"], [data-theme="office"][data-font="classic"] {
  --font-base: "SimSun", "宋体", serif;
  --font-mono: "Courier New", monospace;
}

body {
  font-family: var(--font-base);
}
`;

css += globalFontStyles;

// Step 2: Replace hardcoded font families
css = css.replace(/font-family:\s*'Inter',\s*'Noto Sans SC',\s*sans-serif;/g, 'font-family: var(--font-base);');
css = css.replace(/font-family:\s*"DM Mono",\s*'Noto Sans SC',\s*sans-serif;/g, 'font-family: var(--font-base);');
css = css.replace(/font-family:\s*"DM Mono",\s*Consolas,\s*'Noto Sans SC',\s*sans-serif;/g, 'font-family: var(--font-mono);');
css = css.replace(/font-family:\s*'DM Mono',\s*'Noto Sans SC',\s*monospace/g, 'font-family: var(--font-mono);');
css = css.replace(/font-family:\s*'DM Mono',\s*Consolas,\s*'Noto Sans SC',\s*monospace/g, 'font-family: var(--font-mono);');

// Handle single quote / double quote variations that were introduced
css = css.replace(/font-family:\s*'DM Mono',\s*'Noto Sans SC',\s*monospace;/g, 'font-family: var(--font-mono);');
css = css.replace(/font-family:\s*'DM Mono',\s*Consolas,\s*'Noto Sans SC',\s*monospace;/g, 'font-family: var(--font-mono);');
css = css.replace(/font-family:\s*"DM Mono",\s*monospace;/g, 'font-family: var(--font-mono);');

fs.writeFileSync(cssPath, css, 'utf8');
console.log('App.css updated with CSS variables for fonts.');
