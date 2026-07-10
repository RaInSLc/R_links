const fs = require('fs');
let p = 'src/App.test.tsx';
let c = fs.readFileSync(p, 'utf8');

if(!c.includes('@tauri-apps/plugin-clipboard-manager')) {
    c = c.replace(
        "import * as tauriCore from '@tauri-apps/api/core';",
        "import * as tauriCore from '@tauri-apps/api/core';\nimport { writeText } from '@tauri-apps/plugin-clipboard-manager';"
    );
    c = c.replace(
        "vi.mock('@tauri-apps/api/core', () => ({\n  invoke: vi.fn(),\n}));",
        "vi.mock('@tauri-apps/api/core', () => ({\n  invoke: vi.fn(),\n}));\n\nvi.mock('@tauri-apps/plugin-clipboard-manager', () => ({\n  writeText: vi.fn(),\n}));"
    );
}

c = c.replace(/vi\.mocked\(navigator\.clipboard\.writeText\)/g, 'vi.mocked(writeText)');
fs.writeFileSync(p, c, 'utf8');
console.log('Fixed App.test.tsx');
