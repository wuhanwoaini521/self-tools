import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';

const root = process.cwd();
const git = (...args) => execFileSync('git', args, { cwd: root, encoding: 'utf8' });
const head = git('rev-parse', 'HEAD').trim();
const tracked = git('diff', '--name-status', 'HEAD', '--').trim().split('\n').filter(Boolean);
const untracked = git('ls-files', '--others', '--exclude-standard').trim().split('\n').filter(Boolean);
const excludedRoots = ['artifacts/', 'config/', 'output/', 'target/', 'node_modules/', 'apps/desktop/node_modules/', 'apps/desktop/ui/node_modules/', 'apps/desktop/ui/dist/', 'data/raw/', 'data/generated/'];
const excludedFiles = ['mise.local.toml', 'mise.local.lock', '.mise.local.toml', '.mise.local.lock'];
const excluded = path => excludedRoots.some(prefix => path.startsWith(prefix)) || excludedFiles.includes(path);
const sha256 = path => createHash('sha256').update(readFileSync(resolve(root, path))).digest('hex');
const entries = [];
for (const line of tracked) {
  const [status, ...names] = line.split('\t');
  const path = names.at(-1);
  if (excluded(path)) continue;
  entries.push({ path, status, sha256: status === 'D' ? null : sha256(path) });
}
for (const path of untracked) {
  if (excluded(path)) continue;
  entries.push({ path, status: '??', sha256: sha256(path) });
}
entries.sort((a, b) => a.path.localeCompare(b.path));
const diff = execFileSync('git', ['diff', '--binary', 'HEAD', '--'], { cwd: root });
const submodule = git('submodule', 'status').trim().split('\n').filter(Boolean);
const statusLines = git('status', '--porcelain=v1').trimEnd().split('\n').filter(Boolean);
const manifest = {
  schema: 'self-tools-release-source-manifest/v1',
  capturedAt: new Date().toISOString(),
  classification: 'mutable local candidate overlay; not a Git commit or immutable release',
  repository: { root, branch: git('branch', '--show-current').trim(), head, submodules: submodule },
  worktree: {
    statusEntries: statusLines.length,
    trackedDiffSha256: createHash('sha256').update(diff).digest('hex'),
    stagedChanges: git('diff', '--cached', '--name-only').trim().length > 0,
    changedInputs: entries,
    excluded: [...excludedRoots, ...excludedFiles].map(path => ({ path, reason: path.startsWith('config/') ? 'private local configuration and credentials' : path.startsWith('artifacts/') ? 'review evidence and generated screenshots, not build source' : 'generated, local, or downloaded data/dependency output' })),
  },
  toolchain: {
    mise: execFileSync('mise', ['--version'], { cwd: root, encoding: 'utf8' }).trim(),
    node: execFileSync('node', ['--version'], { cwd: root, encoding: 'utf8' }).trim(),
    npm: execFileSync('npm', ['--version'], { cwd: root, encoding: 'utf8' }).trim(),
    rustc: execFileSync('rustc', ['--version'], { cwd: root, encoding: 'utf8' }).trim(),
    cargo: execFileSync('cargo', ['--version'], { cwd: root, encoding: 'utf8' }).trim(),
    tauri: execFileSync('npm', ['--prefix', 'apps/desktop/ui', 'exec', '--', 'tauri', '--version'], { cwd: root, encoding: 'utf8' }).trim(),
    platform: execFileSync('uname', ['-sm'], { cwd: root, encoding: 'utf8' }).trim(),
    macOS: execFileSync('sw_vers', ['-productVersion'], { cwd: root, encoding: 'utf8' }).trim(),
  },
  buildEntryPoints: {
    rustCheck: 'cargo check --workspace --all-targets --locked',
    rustTests: 'cargo test --workspace --locked',
    clippy: 'cargo clippy --workspace --all-targets --all-features --locked -- -D warnings',
    rustFmt: 'cargo fmt --all -- --check',
    uiBuild: 'npm --prefix apps/desktop/ui run build',
    uiTests: 'npm --prefix apps/desktop/ui test',
    nativeE2E: 'npm --prefix apps/desktop run test:e2e',
    release: 'npm --prefix apps/desktop/ui exec -- tauri build --bundles app --no-sign',
  },
  note: 'The manifest records HEAD plus the hash of the unstaged tracked diff and SHA-256 for each changed/untracked source input. It intentionally excludes ignored local data, credentials, downloaded dependencies, and artifact evidence. A rebuild from clean Git requires a separately authorized, clean commit baseline.',
};
writeFileSync(resolve(root, 'artifacts/ui-review/sprint-11.1/source-manifest.json'), JSON.stringify(manifest, null, 2) + '\n');
console.log(JSON.stringify({ output: 'artifacts/ui-review/sprint-11.1/source-manifest.json', trackedDiffSha256: manifest.worktree.trackedDiffSha256, entries: entries.length, statusEntries: statusLines.length, staged: manifest.worktree.stagedChanges }, null, 2));
