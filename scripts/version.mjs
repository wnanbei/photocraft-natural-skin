// Version management for this single-package repository. Node.js core modules only.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';

export const projectRoot = fileURLToPath(new URL('../', import.meta.url));
const packageName = 'photocraft-natural-skin';
const semver = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?(?:\+([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?$/;

export function parseVersion(value) {
  assert.equal(typeof value, 'string', 'Version must be a string');
  const match = value.match(semver);
  assert.ok(match && value.length <= 32, `Invalid version: ${value} (SemVer, max 32 characters)`);
  const core = match.slice(1, 4).map(Number);
  assert.ok(core.every(Number.isSafeInteger), 'Version components exceed the supported integer range');
  const prerelease = match[4]?.split('.') || [];
  assert.ok(prerelease.every(x => !/^\d+$/.test(x) || x === '0' || !x.startsWith('0')),
    'Numeric prerelease identifiers cannot have leading zeroes');
  return { core, prerelease, metadata: match[5] || '' };
}

export function compareVersions(left, right) {
  const a = parseVersion(left), b = parseVersion(right);
  for (let i = 0; i < 3; i++) if (a.core[i] !== b.core[i]) return a.core[i] > b.core[i] ? 1 : -1;
  if (!a.prerelease.length || !b.prerelease.length) {
    return a.prerelease.length === b.prerelease.length ? 0 : a.prerelease.length ? -1 : 1;
  }
  for (let i = 0; i < Math.max(a.prerelease.length, b.prerelease.length); i++) {
    const x = a.prerelease[i], y = b.prerelease[i];
    if (x === undefined || y === undefined) return x === undefined ? -1 : 1;
    if (x === y) continue;
    const xn = /^\d+$/.test(x), yn = /^\d+$/.test(y);
    if (xn && yn) return BigInt(x) > BigInt(y) ? 1 : -1;
    if (xn !== yn) return xn ? -1 : 1;
    return x > y ? 1 : -1;
  }
  return 0; // Build metadata does not affect SemVer precedence.
}

export function nextVersion(current, change) {
  const { core: [major, minor, patch], prerelease } = parseVersion(current);
  let next;
  switch (change) {
    case 'patch': next = `${major}.${minor}.${patch + (prerelease.length ? 0 : 1)}`; break;
    case 'minor': next = `${major}.${minor + (prerelease.length && patch === 0 ? 0 : 1)}.0`; break;
    case 'major': next = `${major + (prerelease.length && minor === 0 && patch === 0 ? 0 : 1)}.0.0`; break;
    default: next = change;
  }
  parseVersion(next);
  assert.ok(compareVersions(next, current) > 0, 'New version must have higher precedence than the current version');
  return next;
}

function table(text, header) {
  const start = text.indexOf(header);
  assert.ok(start >= 0, `Missing TOML table ${header}`);
  const bodyStart = start + header.length;
  const next = text.slice(bodyStart).search(/^\[/m);
  const end = next < 0 ? text.length : bodyStart + next;
  return { start, end, text: text.slice(start, end) };
}

function value(text, key) {
  const match = text.match(new RegExp(`^${key}\\s*=\\s*"([^"\\r\\n]+)"`, 'm'));
  assert.ok(match, `Missing TOML ${key}`);
  return match[1];
}

function lockPackage(text) {
  const starts = [...text.matchAll(/^\[\[package\]\]\s*$/gm)].map(x => x.index);
  for (let i = 0; i < starts.length; i++) {
    const start = starts[i], end = starts[i + 1] ?? text.length;
    const block = text.slice(start, end);
    if (value(block, 'name') === packageName) return { start, end, text: block };
  }
  throw new Error(`Cargo.lock is missing ${packageName}`);
}

function setTableVersion(text, block, next) {
  const updated = block.text.replace(/^(version\s*=\s*")[^"]+(".*)$/m,
    (_match, prefix, suffix) => `${prefix}${next}${suffix}`);
  assert.notEqual(updated, block.text, 'Version replacement did not change the TOML table');
  return text.slice(0, block.start) + updated + text.slice(block.end);
}

function entries(changelog) {
  const matches = [...changelog.matchAll(/^## \[([^\]]+)\] - ([^\r\n]+)\r?$/gm)];
  assert.ok(matches.length, 'CHANGELOG.md needs a version section');
  return matches.map((match, i) => ({ version: match[1], date: match[2], start: match.index,
    heading: match[0], end: matches[i + 1]?.index ?? changelog.length,
    body: changelog.slice(match.index + match[0].length, matches[i + 1]?.index ?? changelog.length).trim() }));
}

function validDate(date) {
  return /^\d{4}-\d{2}-\d{2}$/.test(date) && Number.isFinite(Date.parse(date))
    && new Date(date).toISOString().slice(0, 10) === date;
}

export function checkProject(root = projectRoot, { tag, release = false } = {}) {
  const files = Object.fromEntries(['Cargo.toml', 'Cargo.lock', 'manifest.json', 'CHANGELOG.md']
    .map(name => [name, fs.readFileSync(path.join(root, name), 'utf8')]));
  const cargo = table(files['Cargo.toml'], '[package]');
  assert.equal(value(cargo.text, 'name'), packageName);
  const version = value(cargo.text, 'version');
  const parsed = parseVersion(version);
  const manifest = JSON.parse(files['manifest.json']);
  assert.equal(manifest.version, version, 'Cargo.toml and manifest.json versions differ');
  const lock = lockPackage(files['Cargo.lock']);
  assert.equal(value(lock.text, 'version'), version, 'Cargo.toml and Cargo.lock versions differ');
  const history = entries(files['CHANGELOG.md']);
  assert.equal(history[0].version, version, 'The top CHANGELOG.md section must match the Cargo version');
  const seen = new Set();
  for (let i = 0; i < history.length; i++) {
    const entry = history[i];
    parseVersion(entry.version);
    assert.ok(!seen.has(entry.version), `Duplicate changelog version ${entry.version}`);
    seen.add(entry.version);
    assert.ok(validDate(entry.date) || (i === 0 && entry.date === 'Unreleased'),
      `Invalid changelog date for ${entry.version}`);
    if (i > 0) assert.ok(compareVersions(history[i - 1].version, entry.version) > 0,
      'Changelog versions must be ordered newest first');
  }
  if (tag !== undefined) assert.equal(tag, `v${version}`, 'Git tag must match the Cargo version exactly');
  if (release) {
    assert.ok(validDate(history[0].date), 'Release changelog must have a YYYY-MM-DD date, not Unreleased');
    const content = history[0].body.replace(/^#{1,6} .*$/gm, '').trim();
    assert.ok(content && !/\b(?:TODO|TBD)\b/i.test(content), 'Release notes need substantive changes, without TODO/TBD placeholders');
  }
  return { version, channel: parsed.prerelease.length ? 'prerelease' : 'stable',
    pending: history[0].date === 'Unreleased', files, cargo, lock, manifest, history };
}

// Stage every edit before replacing files. On a normal filesystem failure, restore replaced
// files so a failed bump does not leave Cargo/manifest/changelog partially synchronized.
function writeChanges(root, changes) {
  const journal = Object.entries(changes).map(([name, content]) => ({
    target: path.join(root, name), content, original: fs.readFileSync(path.join(root, name)),
    temp: path.join(root, `${name}.version-${randomUUID()}`), replaced: false,
  }));
  try {
    for (const file of journal) fs.writeFileSync(file.temp, file.content, { flag: 'wx' });
    for (const file of journal) { fs.renameSync(file.temp, file.target); file.replaced = true; }
  } catch (error) {
    for (const file of journal) if (file.replaced) fs.writeFileSync(file.target, file.original);
    throw error;
  } finally {
    for (const file of journal) fs.rmSync(file.temp, { force: true });
  }
}

export function bumpProject(root, change) {
  const state = checkProject(root);
  const next = nextVersion(state.version, change);
  const top = state.history[0], changelog = state.files['CHANGELOG.md'];
  const newChangelog = state.pending
    ? changelog.slice(0, top.start) + changelog.slice(top.start).replace(top.heading, `## [${next}] - Unreleased`)
    : changelog.slice(0, top.start) + `## [${next}] - Unreleased\n\n` + changelog.slice(top.start);
  const manifest = { ...state.manifest, version: next };
  writeChanges(root, {
    'Cargo.toml': setTableVersion(state.files['Cargo.toml'], state.cargo, next),
    'Cargo.lock': setTableVersion(state.files['Cargo.lock'], state.lock, next),
    'manifest.json': JSON.stringify(manifest, null, 2) + '\n',
    'CHANGELOG.md': newChangelog,
  });
  return checkProject(root);
}

export function prepareRelease(root, date) {
  const state = checkProject(root);
  assert.ok(state.pending, 'This version already has a release date; bump before preparing another version');
  assert.ok(validDate(date), 'Expected a real YYYY-MM-DD date');
  // Validate release content before mutating any file.
  const content = state.history[0].body.replace(/^#{1,6} .*$/gm, '').trim();
  assert.ok(content && !/\b(?:TODO|TBD)\b/i.test(content), 'Add release notes before preparing a release');
  const changelog = state.files['CHANGELOG.md'].replace(state.history[0].heading, `## [${state.version}] - ${date}`);
  writeChanges(root, { 'CHANGELOG.md': changelog });
  return checkProject(root, { release: true });
}

export function releaseNotes(state) {
  return `# Natural Skin ${state.version}${state.pending ? ' (Unreleased)' : ''}\n\n${state.history[0].body}\n`;
}

function main(args) {
  const [command = 'current', ...rest] = args;
  if (command === 'current' || command === 'channel') {
    assert.equal(rest.length, 0, 'Unexpected arguments');
    console.log(checkProject()[command === 'channel' ? 'channel' : 'version']);
  } else if (command === 'check') {
    const options = {};
    while (rest.length) {
      const arg = rest.shift();
      if (arg === '--release') options.release = true;
      else if (arg === '--tag') { assert.ok(rest.length, '--tag requires a value'); options.tag = rest.shift(); }
      else throw new Error(`Unknown option ${arg}`);
    }
    const state = checkProject(projectRoot, options);
    console.log(`Version ${state.version}: synchronized (${state.channel}, ${state.pending ? 'unreleased' : 'dated'})`);
  } else if (command === 'bump') {
    assert.equal(rest.length, 1, 'Usage: node scripts/version.mjs bump patch|minor|major|<version>');
    console.log(`Prepared ${bumpProject(projectRoot, rest[0]).version}; update its changelog before release.`);
  } else if (command === 'prepare') {
    let date;
    if (!rest.length) {
      const now = new Date();
      date = `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, '0')}-${String(now.getDate()).padStart(2, '0')}`;
    } else {
      assert.ok(rest.length === 2 && rest[0] === '--date', 'Usage: node scripts/version.mjs prepare [--date YYYY-MM-DD]');
      date = rest[1];
    }
    const state = prepareRelease(projectRoot, date);
    console.log(`Prepared v${state.version} (${date}). No commit, tag or push was created.`);
  } else if (command === 'notes') {
    assert.equal(rest.length, 0, 'Unexpected arguments');
    process.stdout.write(releaseNotes(checkProject(projectRoot, { release: true })));
  } else {
    throw new Error('Commands: current, channel, check [--tag vX.Y.Z] [--release], bump patch|minor|major|<version>, prepare [--date YYYY-MM-DD], notes');
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try { main(process.argv.slice(2)); }
  catch (error) { console.error(`Version error: ${error.message}`); process.exitCode = 1; }
}
