import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { parseVersion, compareVersions, nextVersion, checkProject, bumpProject,
  prepareRelease, releaseNotes } from '../scripts/version.mjs';

function fixture(t, version = '1.0.0', date = 'Unreleased') {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'natural-skin-version-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  fs.writeFileSync(path.join(root, 'Cargo.toml'), `[package]\nname = "photocraft-natural-skin"\nversion = "${version}"\n\n[dependencies]\nexample = "2.0"\n`);
  fs.writeFileSync(path.join(root, 'Cargo.lock'), `version = 4\n\n[[package]]\nname = "example"\nversion = "2.0.0"\n\n[[package]]\nname = "photocraft-natural-skin"\nversion = "${version}"\n`);
  fs.writeFileSync(path.join(root, 'manifest.json'), JSON.stringify({ id: 'test', version, params: { Strength: { default: 45 } } }, null, 2) + '\n');
  fs.writeFileSync(path.join(root, 'CHANGELOG.md'), `# Changelog\n\n## [${version}] - ${date}\n\n- Preserve image texture.\n`);
  return root;
}
const snapshot = root => Object.fromEntries(fs.readdirSync(root).map(name => [name, fs.readFileSync(path.join(root, name), 'utf8')]));

test('accept SemVer including prereleases and metadata, reject malformed or host-incompatible versions', () => {
  for (const version of ['0.0.0', '1.0.0', '2.3.4-alpha.1', '2.3.4-beta.2', '2.3.4-rc.1+build.7']) parseVersion(version);
  for (const version of ['v1.0.0', '1.0', '01.0.0', '1.0.0-01', '1.0.0-alpha..1', '1.0.0+', '1.0.0-abcdefghijklmnopqrstuvwxyza']) {
    assert.throws(() => parseVersion(version), /version|Version|Numeric/);
  }
});

test('SemVer precedence orders alpha, beta, rc, stable and numeric prerelease identifiers correctly', () => {
  const ordered = ['1.0.0-alpha', '1.0.0-alpha.1', '1.0.0-alpha.2', '1.0.0-alpha.10',
    '1.0.0-alpha.beta', '1.0.0-beta.2', '1.0.0-rc.1', '1.0.0', '1.0.1', '1.1.0', '2.0.0'];
  for (let i = 1; i < ordered.length; i++) assert.equal(compareVersions(ordered[i - 1], ordered[i]), -1);
  assert.equal(compareVersions('1.0.0+build.1', '1.0.0+build.2'), 0);
  assert.equal(compareVersions('1.0.0-alpha.90071992547409930', '1.0.0-alpha.90071992547409931'), -1);
});

test('automatic bumps and explicit prerelease promotions are monotonic', () => {
  assert.equal(nextVersion('1.2.3', 'patch'), '1.2.4');
  assert.equal(nextVersion('1.2.3', 'minor'), '1.3.0');
  assert.equal(nextVersion('1.2.3', 'major'), '2.0.0');
  assert.equal(nextVersion('1.3.0-rc.1', 'patch'), '1.3.0');
  assert.equal(nextVersion('1.3.0-rc.1', 'minor'), '1.3.0');
  assert.equal(nextVersion('2.0.0-beta.2', 'major'), '2.0.0');
  assert.equal(nextVersion('1.2.3', '1.3.0-beta.1'), '1.3.0-beta.1');
  for (const version of ['1.2.3', '1.2.2', '1.2.3-beta.1']) assert.throws(() => nextVersion('1.2.3', version), /higher precedence/);
});

test('version check rejects Cargo/manifest/lock/changelog drift and incorrect tags', t => {
  const root = fixture(t);
  assert.equal(checkProject(root).version, '1.0.0');
  const initial = snapshot(root);
  for (const name of ['manifest.json', 'Cargo.lock', 'CHANGELOG.md']) {
    fs.writeFileSync(path.join(root, name), initial[name].replace('1.0.0', '1.0.1'));
    assert.throws(() => checkProject(root), /differ|match/);
    fs.writeFileSync(path.join(root, name), initial[name]);
  }
  assert.throws(() => checkProject(root, { tag: 'v1.0.1' }), /tag must match/);
  assert.throws(() => checkProject(root, { tag: '1.0.0' }), /tag must match/);
  assert.throws(() => checkProject(root, { release: true }), /not Unreleased/);
});

test('bump updates all versioned metadata, preserves manifest controls and dependency locks', t => {
  const root = fixture(t);
  const state = bumpProject(root, 'minor');
  assert.equal(state.version, '1.1.0');
  assert.equal(state.manifest.params.Strength.default, 45);
  assert.match(state.files['Cargo.lock'], /name = "example"\nversion = "2.0.0"/);
  assert.match(state.files['Cargo.toml'], /example = "2.0"/);
  assert.equal(state.history.length, 1, 'An unpublished draft should not become fake release history');
  assert.match(state.history[0].body, /Preserve image texture/);
  assert.equal(state.pending, true);
});

test('bump after a dated release preserves old notes and requires new release notes', t => {
  const root = fixture(t, '1.0.0', '2026-11-06');
  const state = bumpProject(root, 'patch');
  assert.equal(state.version, '1.0.1');
  assert.equal(state.history.length, 2);
  assert.equal(state.history[1].date, '2026-11-06');
  assert.match(state.history[1].body, /Preserve image texture/);
  assert.throws(() => prepareRelease(root, '2026-11-07'), /Add release notes/);
});

test('rejected bump leaves every project file unchanged', t => {
  const root = fixture(t), initial = snapshot(root);
  assert.throws(() => bumpProject(root, '0.9.0'), /higher precedence/);
  assert.deepEqual(snapshot(root), initial);
  fs.writeFileSync(path.join(root, 'manifest.json'), '{"version":"0.9.0"}\n');
  const drifted = snapshot(root);
  assert.throws(() => bumpProject(root, 'minor'), /differ/);
  assert.deepEqual(snapshot(root), drifted);
});

test('a filesystem failure restores already-replaced files and removes staging files', t => {
  const root = fixture(t), initial = snapshot(root);
  const rename = fs.renameSync;
  let calls = 0;
  fs.renameSync = (...args) => {
    if (++calls === 2) throw new Error('Simulated rename failure');
    return rename(...args);
  };
  try { assert.throws(() => bumpProject(root, 'minor'), /Simulated rename failure/); }
  finally { fs.renameSync = rename; }
  assert.deepEqual(snapshot(root), initial);
});

test('prepare dates a release without touching metadata or creating Git state', t => {
  const root = fixture(t), before = snapshot(root);
  const state = prepareRelease(root, '2026-11-06');
  assert.equal(state.history[0].date, '2026-11-06');
  assert.equal(checkProject(root, { tag: 'v1.0.0', release: true }).version, '1.0.0');
  for (const name of ['Cargo.toml', 'Cargo.lock', 'manifest.json']) assert.equal(state.files[name], before[name]);
  assert.ok(!fs.existsSync(path.join(root, '.git')));
  assert.equal(releaseNotes(state), '# Natural Skin 1.0.0\n\n- Preserve image texture.\n');
  assert.throws(() => prepareRelease(root, '2026-11-07'), /already has a release date/);
});

test('prepare rejects impossible dates and placeholder notes without mutations', t => {
  const root = fixture(t), initial = snapshot(root);
  for (const date of ['2026-02-30', '2026-13-01', '2026-1-01', 'not-a-date']) {
    assert.throws(() => prepareRelease(root, date), /real YYYY-MM-DD/);
    assert.deepEqual(snapshot(root), initial);
  }
  fs.writeFileSync(path.join(root, 'CHANGELOG.md'), '# Changelog\n\n## [1.0.0] - Unreleased\n\n- TODO describe change.\n');
  assert.throws(() => prepareRelease(root, '2026-11-06'), /Add release notes/);
});

test('prerelease releases are detected independently of build metadata', t => {
  const root = fixture(t, '1.1.0-rc.1');
  assert.equal(prepareRelease(root, '2026-11-06').channel, 'prerelease');
  assert.equal(checkProject(root, { tag: 'v1.1.0-rc.1', release: true }).channel, 'prerelease');
});

test('reject duplicate, unordered and invalid historical changelog entries', t => {
  const root = fixture(t), current = fs.readFileSync(path.join(root, 'CHANGELOG.md'), 'utf8');
  for (const [entry, pattern] of [
    ['## [1.0.0] - 2026-11-05\n\n- Duplicate.', /Duplicate/],
    ['## [1.1.0] - 2026-11-05\n\n- Future.', /newest first/],
    ['## [0.9.0] - Unreleased\n\n- Old draft.', /Invalid changelog date/],
  ]) {
    fs.writeFileSync(path.join(root, 'CHANGELOG.md'), current + '\n' + entry + '\n');
    assert.throws(() => checkProject(root), pattern);
  }
});
