const fs = require('fs');
const path = require('path');
const cssPath = path.join(__dirname, 'mod_UI/src/App.css');
let css = fs.readFileSync(cssPath, 'utf8');

// Replace standard fonts
css = css.replace(/"Segoe UI", "Microsoft YaHei UI", sans-serif/g, "'Inter', 'Noto Sans SC', sans-serif");
css = css.replace(/"Microsoft YaHei UI", sans-serif/g, "'Noto Sans SC', sans-serif");
css = css.replace(/font-family: inherit;/g, "font-family: 'Inter', 'Noto Sans SC', sans-serif;");

// Fix DM Mono references to include Noto Sans SC fallback
css = css.replace(/"DM Mono", "Microsoft YaHei UI", sans-serif/g, "'DM Mono', 'Noto Sans SC', monospace");
css = css.replace(/"DM Mono", Consolas, "Microsoft YaHei UI", sans-serif/g, "'DM Mono', Consolas, 'Noto Sans SC', monospace");

// Bump sizes further
css = css.replace(/font-size: 11px;/g, 'font-size: 13px;');
css = css.replace(/font-size: 12px;/g, 'font-size: 13px;');
css = css.replace(/font-size: 13px;/g, 'font-size: 14px;');
css = css.replace(/font-size: 14px;/g, 'font-size: 15px;');

fs.writeFileSync(cssPath, css, 'utf8');
console.log('App.css font families updated successfully');
