// Node.js core modules only. Run after the release WASM build.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';
import assert from 'node:assert/strict';
import { checkProject, releaseNotes } from './version.mjs';

const root = fileURLToPath(new URL('../', import.meta.url));
assert.ok(process.argv.length <= 3, 'Usage: node scripts/package.mjs [v<version>]');
const state = checkProject(root, { tag: process.argv[2], release: Boolean(process.argv[2]) });
const { version } = state;

const wasm = fs.readFileSync(path.join(root,
  'target/wasm32-unknown-unknown/release/photocraft_natural_skin.wasm'));
const module = new WebAssembly.Module(wasm);
assert.deepEqual(WebAssembly.Module.imports(module), [], 'A plugin must have no external imports');
const ex = new WebAssembly.Instance(module).exports;
assert.equal(ex.pc_abi_version(), 1);
for (const key of ['pc_alloc', 'pc_filter', 'pc_manifest']) assert.equal(typeof ex[key], 'function');
const packed = ex.pc_manifest();
const embedded = JSON.parse(new TextDecoder().decode(new Uint8Array(ex.memory.buffer,
  Number(packed & 0xffffffffn), Number(packed >> 32n))));
const manifest = JSON.parse(fs.readFileSync(path.join(root, 'manifest.json'), 'utf8'));
assert.deepEqual(embedded, manifest, 'Stale binary: rebuild after editing manifest.json');
assert.equal(manifest.version, version, 'Cargo and manifest versions must match');
assert.equal(manifest.id, 'io.github.wnanbei.photocraft.natural-skin');

const out = path.join(root, 'dist');
fs.mkdirSync(out, { recursive: true });
const filename = `NaturalSkin-${version}.wasm`;
// Only remove old versioned plugin files managed by this packager, not unrelated files.
for (const old of fs.readdirSync(out)) {
  if (old !== filename && /^NaturalSkin-\d+\.\d+\.\d+[^/]*\.wasm$/.test(old)) {
    fs.rmSync(path.join(out, old));
  }
}
fs.writeFileSync(path.join(out, filename), wasm);
for (const filename of ['README.md', 'README.zh-CN.md', 'LICENSE', 'CHANGELOG.md', 'manifest.json']) {
  fs.copyFileSync(path.join(root, filename), path.join(out, filename));
}
fs.writeFileSync(path.join(out, 'RELEASE_NOTES.md'), releaseNotes(state));
const assets = [filename, 'README.md', 'README.zh-CN.md', 'LICENSE', 'CHANGELOG.md', 'manifest.json', 'RELEASE_NOTES.md'];
const sums = assets.map(name => `${createHash('sha256').update(fs.readFileSync(path.join(out, name))).digest('hex')}  ${name}`);
fs.writeFileSync(path.join(out, 'SHA256SUMS.txt'), sums.join('\n') + '\n');
console.log(`Packaged ${filename} (${wasm.length} bytes) in dist/`);
