import { build } from 'esbuild';
import { mkdir, copyFile, writeFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
const root = fileURLToPath(new URL('.', import.meta.url));
process.chdir(root);
await mkdir('dist', { recursive: true });
await build({ entryPoints: ['src/app.js'], bundle: true, format: 'esm', target: 'es2022', outdir: 'dist', minify: true, loader: { '.ttf': 'file' }, assetNames: 'assets/[name]-[hash]', legalComments: 'linked' });
await build({ entryPoints: ['node_modules/monaco-editor/esm/vs/editor/editor.worker.js'], bundle: true, format: 'iife', target: 'es2022', outfile: 'dist/editor.worker.js', minify: true, legalComments: 'linked' });
await copyFile('src/index.html', 'dist/index.html');
await mkdir('dist/licenses', { recursive: true });
for (const [name, license] of [['monaco-editor','LICENSE'],['marked','LICENSE'],['dompurify','LICENSE'],['dompurify','LICENSE-MPL']]) {
  await copyFile(`node_modules/${name}/${license}`, `dist/licenses/${name}-${license}.txt`);
}
await writeFile('dist/build-info.json', JSON.stringify({ editor: 'Monaco', offline: true }));
console.log('Offline editor bundled in dist/');
