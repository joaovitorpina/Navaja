// Spike S2.2 (docs/spikes.md): does Vite hot-reload a custom view that lives
// in tools/<id>/ui/, outside app/?
//
// Needs the probe from s2-2-probe.sh and `pnpm install`. Run it from anywhere:
//   node scripts/spikes/hmr-check.mjs
//
// It starts the dev server itself: app/'s own `vite`, the one `pnpm dev`
// starts through Tauri, with app/vite.config.ts, but on a free port of
// 127.0.0.1 instead of 1420. Then it does what the webview does when the
// shell opens the probe, without a webview:
// 1. asks for ToolHost.svelte, takes the probe view's URL from its
//    import.meta.glob, and asks for the view and the label.ts it imports.
//    Only then are both files in Vite's module graph and watched: Vite
//    watches files outside its root (app/) once it has served them.
// 2. connects to Vite's HMR WebSocket (subprotocol `vite-hmr`).
// 3. edits View.svelte, and expects an `update` that names it within the
//    timeout. It then asks for the module at that update's timestamp, as the
//    client would, and expects the edit in it.
// 4. edits label.ts and expects an `update` again. label.ts does not accept
//    updates itself, so Vite names the module that does, the view that imports
//    it. The view at that update's timestamp must import label.ts with a new
//    `?t=` stamp, and label.ts at that URL must hold the edit.
// No update in time fails the check, and so does a `full-reload` that Vite
// sends because of the edit (its fallback when nothing accepts the update).
//
// It restores both files and stops the server, pass or fail, and exits
// non-zero on failure. HMR_CHECK_TIMEOUT_MS sets the timeout per edit
// (default 20000).
import { spawn } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:net';
import { join } from 'node:path';
import { setTimeout as sleep } from 'node:timers/promises';
import { fileURLToPath } from 'node:url';

const repo = fileURLToPath(new URL('../../', import.meta.url));
const app = join(repo, 'app');
const ui = join(repo, 'tools', 'probe', 'ui');
const files = { view: join(ui, 'View.svelte'), label: join(ui, 'label.ts') };
const VIEW_PATH = '/tools/probe/ui/View.svelte';
const LABEL_PATH = '/tools/probe/ui/label.ts';
const TIMEOUT = Number(process.env.HMR_CHECK_TIMEOUT_MS) || 20_000;
const START_TIMEOUT = 60_000;

/** Original contents, restored on exit. */
const originals = new Map();
/** Every HMR message, with the time it arrived. */
const messages = [];
let server;
let socket;
let serverOutput = '';

function fail(message) {
  throw new Error(message);
}

/** A port on 127.0.0.1 that nothing listens on right now. */
function freePort() {
  return new Promise((resolve, reject) => {
    const probe = createServer();
    probe.once('error', reject);
    probe.listen(0, '127.0.0.1', () => {
      const { port } = probe.address();
      probe.close(() => resolve(port));
    });
  });
}

/** Starts app/'s Vite dev server and resolves with its base URL once it answers. */
async function startVite() {
  const vite = join(app, 'node_modules', 'vite');
  const { bin } = JSON.parse(readFileSync(join(vite, 'package.json'), 'utf8'));
  const port = await freePort();
  const args = [join(vite, typeof bin === 'string' ? bin : bin.vite)];
  args.push('--host', '127.0.0.1', '--port', String(port), '--strictPort');
  server = spawn(process.execPath, args, { cwd: app, stdio: ['ignore', 'pipe', 'pipe'] });
  server.stdout.on('data', (chunk) => (serverOutput += chunk));
  server.stderr.on('data', (chunk) => (serverOutput += chunk));

  const base = `http://127.0.0.1:${port}`;
  const deadline = Date.now() + START_TIMEOUT;
  while (Date.now() < deadline) {
    if (server.exitCode !== null)
      fail(`Vite exited with code ${server.exitCode} before it answered`);
    try {
      const response = await fetch(`${base}/`);
      if (response.ok) return base;
    } catch {
      // Not listening yet.
    }
    await sleep(250);
  }
  fail(`Vite did not answer on ${base} within ${START_TIMEOUT} ms`);
}

async function get(base, url) {
  const response = await fetch(new URL(url, base));
  if (!response.ok) fail(`GET ${url}: ${response.status} ${response.statusText}`);
  return response.text();
}

/**
 * The module URLs that `code`, as Vite serves it, imports (`from "…"`,
 * `import "…"`, `import("…")`) and whose path ends in `path`.
 */
function urlsOf(code, path) {
  const imports = /(?:\bfrom\s*|\bimport\s*\(?\s*)(["'])([^"'\s]+)\1/g;
  const found = [];
  for (const [, , url] of code.matchAll(imports)) {
    if (url.split('?')[0].endsWith(path)) found.push(url);
  }
  return found;
}

/** The URL the Vite client imports for an update: the path plus `t=` (vite/client). */
function updateUrl(update) {
  const [path, query] = update.acceptedPath.split('?');
  const explicit = update.explicitImportRequired ? 'import&' : '';
  return `${path}?${explicit}t=${update.timestamp}${query ? `&${query}` : ''}`;
}

/** Waits until no HMR message has arrived for `quiet` ms, at most `limit` ms. */
async function settle(quiet = 1_000, limit = 10_000) {
  const until = Date.now() + limit;
  while (Date.now() < until) {
    const last = messages.at(-1)?.at ?? 0;
    if (Date.now() - last >= quiet) return;
    await sleep(100);
  }
}

/**
 * Resolves with the first `update` after index `from` that lists a module
 * whose path ends in one of `paths`. Fails on an `error`, on a `full-reload`
 * that Vite sends because one of `paths` changed (its HMR fallback names the
 * file in `triggeredBy`), or on no such update within the timeout. Other
 * reloads, such as the dependency optimizer's, are logged and ignored.
 */
async function waitForUpdate(from, paths) {
  const deadline = Date.now() + TIMEOUT;
  let seen = from;
  while (Date.now() < deadline) {
    for (; seen < messages.length; seen++) {
      const { data, at } = messages[seen];
      if (data.type === 'full-reload') {
        const cause = String(data.triggeredBy ?? '').replaceAll('\\', '/');
        if (paths.some((p) => cause.endsWith(p))) {
          fail(`Vite reloaded the page instead of updating it: ${JSON.stringify(data)}`);
        }
        console.log(`Ignored a full reload the edit did not cause: ${JSON.stringify(data)}`);
      }
      if (data.type === 'error') fail(`Vite reported an error: ${JSON.stringify(data.err)}`);
      if (data.type !== 'update') continue;
      const update = data.updates.find((u) => paths.some((p) => u.path.split('?')[0].endsWith(p)));
      if (update) return { update, at };
    }
    await sleep(50);
  }
  fail(`no HMR update naming ${paths.join(' or ')} within ${TIMEOUT} ms`);
}

/** Appends `text` to `file`, keeping the original for restore(). */
function edit(file, text) {
  if (!originals.has(file)) originals.set(file, readFileSync(file, 'utf8'));
  writeFileSync(file, originals.get(file) + text);
  return Date.now();
}

function restore() {
  for (const [file, text] of originals) writeFileSync(file, text);
}

async function stopVite() {
  if (!server || server.exitCode !== null) return;
  const exited = new Promise((resolve) => server.once('exit', resolve));
  server.kill();
  const stopped = await Promise.race([exited.then(() => true), sleep(10_000, false)]);
  if (!stopped) server.kill('SIGKILL');
}

async function check() {
  for (const file of Object.values(files)) readFileSync(file); // the probe must exist
  const base = await startVite();
  console.log(`Vite dev server for app/ on ${base}.`);

  // 1. What the webview loads when the shell opens the probe.
  const host = await get(base, '/src/shell/ToolHost.svelte');
  const [viewUrl] = urlsOf(host, VIEW_PATH);
  if (!viewUrl) fail(`ToolHost.svelte's import.meta.glob does not list ${VIEW_PATH}`);
  const view = await get(base, viewUrl);
  if (!view.includes('probe-view')) fail(`${viewUrl} is not the compiled probe view`);
  const [labelUrl] = urlsOf(view, LABEL_PATH);
  if (!labelUrl) fail(`the compiled view does not import ${LABEL_PATH}`);
  await get(base, labelUrl);
  console.log(`Served through ToolHost's glob: ${viewUrl}, which imports ${labelUrl}.`);

  // 2. The HMR socket, with the token Vite gives its own client.
  const client = await get(base, '/@vite/client');
  const token = /const wsToken = "([^"]+)"/.exec(client)?.[1];
  if (!token) fail('no WebSocket token in /@vite/client');
  socket = new WebSocket(`${base.replace('http', 'ws')}/?token=${token}`, 'vite-hmr');
  socket.addEventListener('message', (event) => {
    messages.push({ data: JSON.parse(String(event.data)), at: Date.now() });
  });
  await new Promise((resolve, reject) => {
    socket.addEventListener('open', resolve, { once: true });
    socket.addEventListener('error', () => reject(new Error('HMR WebSocket error')), {
      once: true,
    });
  });
  const connected = Date.now() + TIMEOUT;
  while (!messages.some((m) => m.data.type === 'connected')) {
    if (Date.now() > connected) fail(`no \`connected\` message from Vite within ${TIMEOUT} ms`);
    await sleep(50);
  }
  await settle();

  // 3. The view itself.
  const viewMark = `hmr-check-view-${Date.now()}`;
  let from = messages.length;
  let wrote = edit(files.view, `\n<p hidden>${viewMark}</p>\n`);
  let { update, at } = await waitForUpdate(from, [VIEW_PATH]);
  const viewAfter = await get(base, updateUrl(update));
  if (!viewAfter.includes(viewMark)) fail(`${updateUrl(update)} does not hold the edit`);
  console.log(
    `PASS View.svelte: ${update.type} for ${update.path} ${at - wrote} ms after the edit; ` +
      'the module at its timestamp holds the edit.',
  );
  await settle();

  // 4. The view's sibling module.
  const labelMark = `hmr-check-label-${Date.now()}`;
  from = messages.length;
  wrote = edit(files.label, `export const HMR_CHECK = '${labelMark}';\n`);
  ({ update, at } = await waitForUpdate(from, [LABEL_PATH, VIEW_PATH]));
  let labelAfter = updateUrl(update);
  let how = 'the module at its timestamp holds the edit';
  if (!update.path.split('?')[0].endsWith(LABEL_PATH)) {
    // Vite stamps an import with `?t=` only once that module went through HMR.
    const boundary = await get(base, updateUrl(update));
    labelAfter = urlsOf(boundary, LABEL_PATH).find((url) => /[?&]t=\d+/.test(url));
    if (!labelAfter) fail(`${updateUrl(update)} does not import a new ${LABEL_PATH}`);
    how = `that module, at its timestamp, imports ${labelAfter}, which holds the edit`;
  }
  if (!(await get(base, labelAfter)).includes(labelMark)) {
    fail(`${labelAfter} does not hold the edit`);
  }
  console.log(
    `PASS label.ts: ${update.type} for ${update.path} ${at - wrote} ms after the edit; ${how}.`,
  );
}

let status = 0;
try {
  await check();
  console.log('HMR works for a custom view in tools/<id>/ui/.');
} catch (error) {
  status = 1;
  console.error(`::error::HMR check: ${error.message}`);
  console.error('HMR messages:');
  for (const { data } of messages) console.error(`  ${JSON.stringify(data)}`);
  if (serverOutput) console.error(`Vite output:\n${serverOutput}`);
} finally {
  restore();
  socket?.close();
  await stopVite();
}
process.exit(status);
