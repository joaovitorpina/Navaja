// TEMPORARY diagnostic (removed before merge): lists where the end-to-end run
// wrote, to confirm the folder list in data-folder-check.mjs on each OS.
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { dirname, join, relative, sep } from 'node:path';
import { tmpdir } from 'node:os';

const since = Number(readFileSync(process.argv[2], 'utf8'));
const record = JSON.parse(readFileSync(process.argv[3], 'utf8'));
const canary = record.canary;
const needles = [];
for (const enc of ['utf8', 'utf16le']) {
  for (let i = 0; i + 12 <= canary.length; i++)
    needles.push(Buffer.from(canary.slice(i, i + 12), enc));
}
const home = process.env.HOME ?? process.env.USERPROFILE;
const roots =
  process.platform === 'win32'
    ? [process.env.LOCALAPPDATA, process.env.APPDATA]
    : process.platform === 'darwin'
      ? [home, dirname(tmpdir())]
      : [home, '/tmp', '/var/tmp', '/run/user'];
const skip = new Set([
  'node_modules',
  '.cargo',
  '.rustup',
  'hostedtoolcache',
  'Android',
  '.npm',
  '.pnpm-store',
  '.nuget',
  'go',
  '.dotnet',
  'work',
  '.git',
  'Homebrew',
  'pnpm',
  'actions-runner',
  'runners',
]);
const deadline = Date.now() + 240_000;
const groups = new Map();
let seen = 0;
function walk(root, dir, depth) {
  if (Date.now() > deadline || depth > 12) return;
  let entries;
  try {
    entries = readdirSync(dir, { withFileTypes: true });
  } catch {
    return;
  }
  for (const entry of entries) {
    const path = join(dir, entry.name);
    if (entry.isSymbolicLink()) continue;
    if (entry.isDirectory()) {
      if (!skip.has(entry.name)) walk(root, path, depth + 1);
    } else if (entry.isFile()) {
      seen++;
      let info;
      try {
        info = statSync(path);
      } catch {
        continue;
      }
      if (info.mtimeMs < since && info.birthtimeMs < since) continue;
      const key = join(root, ...relative(root, path).split(sep).slice(0, 4));
      const group = groups.get(key) ?? { files: 0, bytes: 0, hits: [] };
      group.files++;
      group.bytes += info.size;
      if (info.size < 256 * 2 ** 20) {
        try {
          const bytes = readFileSync(path);
          if (needles.some((n) => bytes.includes(n))) group.hits.push(path);
        } catch (error) {
          group.hits.push(`(unreadable ${error.code}) ${path}`);
        }
      }
      groups.set(key, group);
    }
  }
}
for (const root of roots.filter(Boolean)) walk(root, root, 0);
console.log(`Swept ${seen} files; changed since the e2e step started:`);
for (const [key, group] of [...groups].sort()) {
  console.log(`${key}: ${group.files} files, ${group.bytes} bytes`);
  for (const hit of group.hits) console.log(`  CANARY: ${hit}`);
}
if (Date.now() > deadline) console.log('(stopped at the time limit)');
