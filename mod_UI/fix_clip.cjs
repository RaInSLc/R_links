const fs = require('fs');
function f(p){
    let c = fs.readFileSync(p,'utf8');
    if(p.includes('ReportView')){
        c = c.replace('import { invoke } from "@tauri-apps/api/core";', 'import { invoke } from "@tauri-apps/api/core";\nimport { writeText } from "@tauri-apps/plugin-clipboard-manager";');
    } else {
        c = c.replace('import type { HistoryRecord } from "./utils";', 'import type { HistoryRecord } from "./utils";\nimport { writeText } from "@tauri-apps/plugin-clipboard-manager";');
    }
    c = c.replace(/navigator\.clipboard\.writeText/g, 'writeText');
    fs.writeFileSync(p, c, 'utf8');
}
f('src/ReportView.tsx');
f('src/HistoryView.tsx');
