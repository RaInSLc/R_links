const fs = require('fs');
const path = require('path');
const cssPath = path.join(__dirname, 'mod_UI/src/App.css');
let css = fs.readFileSync(cssPath, 'utf8');

css = css.replace(/"DM Mono", monospace;/g, '"DM Mono", "Microsoft YaHei UI", sans-serif;');
css = css.replace(/"DM Mono", Consolas, monospace;/g, '"DM Mono", Consolas, "Microsoft YaHei UI", sans-serif;');

// Increase base sizes for better readability
css = css.replace(/font-size: 9px;/g, 'font-size: 11px;');
css = css.replace(/font-size: 10px;/g, 'font-size: 12px;');
css = css.replace(/font-size: 11px;/g, 'font-size: 12px;');
css = css.replace(/font-size: 12px;/g, 'font-size: 13px;');
css = css.replace(/font-size: 13px;/g, 'font-size: 14px;');

// Special fix for the brand strong which uses 14px
css = css.replace(/font-size: 14px;/g, 'font-size: 15px;');

fs.writeFileSync(cssPath, css, 'utf8');
console.log('Fonts fixed successfully');
