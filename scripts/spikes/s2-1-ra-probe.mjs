// Spike S2.1 (docs/spikes.md), the rust-analyzer half: do navigation and
// completion work through `register_tools!` in tools/lib.rs? Its `mod $id;`
// is the only declaration of each tool's module, so an IDE that does not
// expand the macro sees tools/<id>/mod.rs as a file outside the crate.
//
// Needs the rust-analyzer and rust-src components of the toolchain in
// rust-toolchain.toml, and Node 24; no npm packages. From anywhere:
//   rustup component add --toolchain 1.99.0 rust-analyzer rust-src
//   node scripts/spikes/s2-1-ra-probe.mjs <check>
//
// Checks, one per run. Positions are found by searching the text.
//   definition-list     a. Go to definition on `uuid` in the register_tools!
//                          list lands in tools/uuid/mod.rs, and nowhere else.
//   definition-core     b. In tools/uuid/mod.rs, go to definition on `Tool`
//                          (`impl Tool for`) and on `ToolMeta` (`-> ToolMeta`)
//                          lands on their definitions in crates/navaja-core.
//   completion          c. `crate::uuid::` in tools/lib.rs offers TOOL and
//                          UuidGenerator; `navaja_core::` in tools/uuid/mod.rs
//                          offers Tool, ToolMeta and Registry.
//   diagnostics         d. For tools/uuid/mod.rs as it is on disk,
//                          rust-analyzer reports no unlinked file and no
//                          unresolved import, module, extern crate or name
//                          (the codes in UNLINKED below).
//   control-removed     The uuid entry removed from the list, in a throwaway
//                       copy: tools/uuid/mod.rs must be reported as an
//                       unlinked file, and b and c must fail.
//   control-misspelled  The entry misspelled `uiud`, in a throwaway copy: a
//                       must fail.
// Each control first checks that rust-analyzer loaded the copy's workspace:
// in tools/registry_test.rs, which lib.rs declares outside the macro, go to
// definition on `Registry` must land in crates/navaja-core. A rust-analyzer
// that loaded nothing would also report every file unlinked and fail a, b
// and c, so without that check a control could pass on nothing.
//
// Each run starts rust-analyzer as an LSP server over stdio, waits until it
// reports itself quiescent (the workspace loaded), asks, and shuts it down.
// The function that c adds for its completions goes to rust-analyzer as an
// unsaved buffer (didOpen), as an editor sends one; nothing in the checkout
// changes. The controls copy the checkout's files (tracked, and untracked
// but not ignored) to a temporary folder, edit tools/lib.rs there, and
// delete the copy on exit.
//
// rust-analyzer runs without build scripts, proc macros, cargo check, cache
// priming or its own file watcher: it leaves watching to the probe, which
// reports no change. register_tools! is a macro_rules!, which
// rust-analyzer expands itself, and the build scripts would compile Tauri.
// Without proc macros it reports a macro-error at each `#[derive]` from a
// proc macro, and an E0277 where a derived trait is needed; d ignores both.
// Its experimental diagnostics are on: rust-analyzer 1.99.0 reports an
// unresolved import (E0432) or name (E0425) only with them.
//
// Exit codes: 0 the check (or the control) passed, 1 it failed, 2 the probe
// could not ask (rust-analyzer missing, crashed, never quiescent, a search
// found nothing, or a bug in the probe). On a runner, exit 2 also sets the
// step's output `result` to "could not ask", which the job's summary shows
// instead of FAIL. RUST_ANALYZER names another binary.
// S21_TIMEOUT_MS sets how long to wait for quiescence (default 600000); each
// answer may take a tenth of that.
import { execFileSync, spawn } from 'node:child_process';
import {
  appendFileSync,
  cpSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, relative, resolve } from 'node:path';
import { setTimeout as sleep } from 'node:timers/promises';
import { fileURLToPath, pathToFileURL } from 'node:url';

const repo = resolve(fileURLToPath(new URL('../../', import.meta.url)));
const LIB = 'tools/lib.rs';
/** The register_tools! invocation, not the macro_rules! that defines it. */
const LIST = 'register_tools! {';
const ID = 'uuid';
const MOD = `tools/${ID}/mod.rs`;
const MISSPELLED = 'uiud';
const REGISTRY_TEST = 'tools/registry_test.rs';
const CORE = 'crates/navaja-core/src/';
/**
 * The diagnostics that say a file is not analysed as part of its crate, or
 * that a path in it does not resolve, by code: rust-analyzer's own name where
 * it has one, rustc's error code where it uses that.
 */
const UNLINKED = new Map([
  ['unlinked-file', 'file outside the module tree'],
  ['E0432', 'unresolved import'],
  ['E0583', 'unresolved module'],
  ['unresolved-extern-crate', 'unresolved extern crate'],
  ['E0425', 'unresolved name'],
]);
const TIMEOUT = Number(process.env.S21_TIMEOUT_MS) || 600_000;
const ANSWER_TIMEOUT = Math.max(TIMEOUT / 10, 10_000);

/** The probe could not ask: exit 2, never a pass or a fail. */
class ProbeError extends Error {}

/** A rust-analyzer LSP session on `root`. */
class Server {
  constructor(root) {
    this.root = root;
    this.nextId = 1;
    this.pending = new Map();
    this.buffer = Buffer.alloc(0);
    this.status = null;
    /** Every serverStatus, in order, for the log. */
    this.statuses = [];
    /** The version of each open file's buffer, by URI. */
    this.versions = new Map();
    this.closing = false;
    const binary = process.env.RUST_ANALYZER ?? 'rust-analyzer';
    this.process = spawn(binary, [], { cwd: root, stdio: ['pipe', 'pipe', 'inherit'] });
    this.exited = new Promise((ok) => this.process.once('exit', ok));
    this.process.once('error', (error) => this.abort(`cannot run ${binary}: ${error.message}`));
    this.process.once('exit', (code, signal) => {
      if (!this.closing) this.abort(`rust-analyzer exited early (${code ?? signal})`);
    });
    this.process.stdin.on('error', (error) => this.abort(`writing to rust-analyzer: ${error}`));
    this.process.stdout.on('data', (chunk) => this.read(chunk));
  }

  abort(message) {
    this.failure ??= new ProbeError(message);
    for (const { fail } of this.pending.values()) fail(this.failure);
    this.pending.clear();
  }

  read(chunk) {
    this.buffer = Buffer.concat([this.buffer, chunk]);
    for (;;) {
      const end = this.buffer.indexOf('\r\n\r\n');
      if (end < 0) return;
      const header = this.buffer.subarray(0, end).toString('ascii');
      const length = Number(/Content-Length: *(\d+)/i.exec(header)?.[1]);
      if (!Number.isInteger(length)) return this.abort(`bad LSP header ${JSON.stringify(header)}`);
      if (this.buffer.length < end + 4 + length) return;
      const body = this.buffer.subarray(end + 4, end + 4 + length).toString('utf8');
      this.buffer = this.buffer.subarray(end + 4 + length);
      try {
        this.receive(JSON.parse(body));
      } catch (error) {
        return this.abort(`bad LSP message: ${error}`);
      }
    }
  }

  receive(message) {
    if (message.method === undefined) {
      const waiter = this.pending.get(message.id);
      if (!waiter) return;
      this.pending.delete(message.id);
      if (message.error) {
        waiter.fail(new ProbeError(`${waiter.method}: ${JSON.stringify(message.error)}`));
      } else {
        waiter.ok(message.result);
      }
    } else if (message.id !== undefined) {
      // A request from the server, such as a progress token or the file
      // watcher's registration (client/registerCapability). null is a valid
      // answer to each: the client declared no capability that needs more.
      this.send({ id: message.id, result: null });
    } else if (message.method === 'experimental/serverStatus') {
      this.status = message.params;
      this.statuses.push(message.params.quiescent ? 'quiescent' : 'busy');
    }
  }

  send(message) {
    const body = Buffer.from(JSON.stringify({ jsonrpc: '2.0', ...message }), 'utf8');
    this.process.stdin.write(`Content-Length: ${body.length}\r\n\r\n`);
    this.process.stdin.write(body);
  }

  notify(method, params) {
    this.send({ method, params });
  }

  request(method, params, timeout = ANSWER_TIMEOUT) {
    if (this.failure) return Promise.reject(this.failure);
    const id = this.nextId++;
    const answer = new Promise((ok, fail) => this.pending.set(id, { ok, fail, method }));
    this.send({ id, method, params });
    let timer;
    const late = new Promise((_, fail) => {
      timer = setTimeout(
        () => fail(new ProbeError(`${method}: no answer in ${timeout} ms`)),
        timeout,
      );
    });
    return Promise.race([answer, late]).finally(() => clearTimeout(timer));
  }

  uri(rel) {
    return pathToFileURL(join(this.root, rel)).href;
  }

  /** Initialises the server and waits until it has loaded the workspace. */
  async start() {
    const rootUri = pathToFileURL(this.root).href;
    const init = await this.request(
      'initialize',
      {
        processId: process.pid,
        rootUri,
        workspaceFolders: [{ uri: rootUri, name: 'navaja' }],
        capabilities: {
          textDocument: {
            definition: { linkSupport: true },
            completion: { completionItem: { snippetSupport: false } },
            diagnostic: { dynamicRegistration: false },
          },
          // rust-analyzer leaves file watching to the client only when the
          // client can register watchers (this) and `files.watcher` is
          // 'client' (below); otherwise it starts its own watcher. The probe
          // reports no change, so rust-analyzer keeps the tree it loaded.
          workspace: { didChangeWatchedFiles: { dynamicRegistration: true } },
          experimental: { serverStatusNotification: true },
        },
        initializationOptions: {
          cargo: { buildScripts: { enable: false } },
          procMacro: { enable: false },
          checkOnSave: false,
          cachePriming: { enable: false },
          files: { watcher: 'client' },
          diagnostics: { experimental: { enable: true } },
        },
      },
      TIMEOUT,
    );
    this.capabilities = init?.capabilities ?? {};
    this.notify('initialized', {});
    const version = init?.serverInfo?.version ?? '(no version in serverInfo)';
    console.log(`rust-analyzer ${version}, on ${this.root}`);

    const started = Date.now();
    while (!this.status?.quiescent) {
      if (this.failure) throw this.failure;
      if (Date.now() - started > TIMEOUT) {
        throw new ProbeError(`not quiescent in ${TIMEOUT} ms: ${JSON.stringify(this.status)}`);
      }
      await sleep(100);
    }
    const { health, message } = this.status;
    console.log(
      `Quiescent after ${Date.now() - started} ms (statuses: ${this.statuses.join(', ')}), ` +
        `health ${health}${message ? `: ${message}` : ''}`,
    );
    if (health === 'error') throw new ProbeError('rust-analyzer could not load the workspace');
  }

  /** Opens `rel` with `text` as its unsaved buffer, or replaces the buffer if it is open. */
  open(rel, text) {
    const uri = this.uri(rel);
    const version = (this.versions.get(uri) ?? 0) + 1;
    this.versions.set(uri, version);
    if (version === 1) {
      this.notify('textDocument/didOpen', {
        textDocument: { uri, languageId: 'rust', version, text },
      });
    } else {
      this.notify('textDocument/didChange', {
        textDocument: { uri, version },
        contentChanges: [{ text }],
      });
    }
  }

  /**
   * rust-analyzer's own diagnostics for an open file, asked for (LSP pull
   * diagnostics) rather than waited for: it publishes nothing for a file
   * with none, so waiting could not tell "none" from "not yet".
   */
  async diagnostics(rel) {
    if (!this.capabilities.diagnosticProvider) {
      throw new ProbeError(
        'this rust-analyzer offers no pull diagnostics (textDocument/diagnostic)',
      );
    }
    const report = await this.request('textDocument/diagnostic', {
      textDocument: { uri: this.uri(rel) },
    });
    if (report?.kind !== 'full') throw new ProbeError(`diagnostics: ${JSON.stringify(report)}`);
    return report.items;
  }

  /** Each target as its path relative to the root, its line and that line's text. */
  async definition(rel, position) {
    const result = await this.request('textDocument/definition', {
      textDocument: { uri: this.uri(rel) },
      position,
    });
    const list = result == null ? [] : Array.isArray(result) ? result : [result];
    return list.map((l) => this.describe(l.targetUri ?? l.uri, l.targetSelectionRange ?? l.range));
  }

  /** The labels of the completion items. */
  async completion(rel, position) {
    const result = await this.request('textDocument/completion', {
      textDocument: { uri: this.uri(rel) },
      position,
    });
    const items = Array.isArray(result) ? result : (result?.items ?? []);
    return items.map((item) => item.label.trim());
  }

  describe(uri, range) {
    const path = fileURLToPath(uri);
    const inside = relative(this.root, path);
    const rel = (inside.startsWith('..') ? path : inside).replaceAll('\\', '/');
    let text = '';
    try {
      text = readFileSync(path, 'utf8').split('\n')[range.start.line]?.trim() ?? '';
    } catch {
      // A file the probe cannot read: the path and line say enough.
    }
    return { rel, line: range.start.line + 1, text };
  }

  async stop() {
    if (this.process.pid === undefined || this.process.exitCode !== null) return;
    this.closing = true;
    if (!this.failure) {
      try {
        await this.request('shutdown', null, 30_000);
        this.notify('exit', null);
      } catch {
        // Killed below.
      }
    }
    // rust-analyzer's reader thread waits for the end of its input before
    // the process exits.
    this.process.stdin.end();
    const exited = await Promise.race([
      this.exited.then(() => true),
      sleep(10_000, false, { ref: false }),
    ]);
    if (!exited) this.process.kill('SIGKILL');
  }
}

/** The file's text with LF line ends, on which the positions are counted. */
function source(root, rel) {
  return readFileSync(join(root, rel), 'utf8').replaceAll('\r\n', '\n');
}

/**
 * The LSP position (UTF-16 code units, LSP's default) of `needle` in `text`
 * plus `offset`, searching from the end of the first `after`, if given.
 */
function position(text, needle, { after = '', offset = 0 } = {}) {
  const from = after ? text.indexOf(after) : 0;
  if (from < 0) throw new ProbeError(`not found: ${JSON.stringify(after)}`);
  const at = text.indexOf(needle, from + after.length);
  if (at < 0) throw new ProbeError(`not found: ${JSON.stringify(needle)}`);
  const before = text.slice(0, at + offset);
  const line = before.split('\n').length - 1;
  return { line, character: before.length - (before.lastIndexOf('\n') + 1) };
}

/** Whether a line of code declares `item`, such as `trait Tool` (and not `trait ToolX`). */
function declares(line, item) {
  return new RegExp(`\\b${item}\\b`).test(line);
}

function show(label, value) {
  console.log(`  ${label}: ${typeof value === 'string' ? value : JSON.stringify(value)}`);
}

function verdict(name, pass, why = '') {
  console.log(`${pass ? 'PASS' : 'FAIL'}  ${name}${why ? `: ${why}` : ''}`);
  return pass;
}

// a. Definition on the list entry `id` lands in tools/uuid/mod.rs only.
async function definitionList(server, id = ID) {
  const text = source(server.root, LIB);
  server.open(LIB, text);
  const targets = await server.definition(LIB, position(text, `${id},`, { after: LIST }));
  show(`definition of \`${id}\` in register_tools!`, targets);
  const pass = targets.length > 0 && targets.every((t) => t.rel === MOD);
  return verdict(`a. definition on \`${id}\` in register_tools! lands in ${MOD}`, pass);
}

// b. Definitions of navaja-core items, asked for inside tools/uuid/mod.rs.
async function definitionCore(server) {
  const text = source(server.root, MOD);
  server.open(MOD, text);
  const cases = [
    ['Tool', 'impl Tool for', 'impl '.length, 'tool.rs', 'trait Tool'],
    ['ToolMeta', '-> ToolMeta', '-> '.length, 'meta.rs', 'struct ToolMeta'],
  ];
  let pass = true;
  for (const [name, needle, offset, file, item] of cases) {
    const targets = await server.definition(MOD, position(text, needle, { offset }));
    show(`definition of \`${name}\` in ${MOD}`, targets);
    const hit =
      targets.length > 0 &&
      targets.every((t) => t.rel === `${CORE}${file}` && declares(t.text, item));
    pass =
      verdict(`b. definition on \`${name}\` lands on \`${item}\` in ${CORE}${file}`, hit) && pass;
  }
  return pass;
}

// c. Completion through the macro-declared module, and inside it.
async function completion(server) {
  const cases = [
    [LIB, `crate::${ID}::`, ['TOOL', 'UuidGenerator']],
    [MOD, 'navaja_core::', ['Tool', 'ToolMeta', 'Registry']],
  ];
  let pass = true;
  for (const [rel, path, wanted] of cases) {
    // An unsaved edit: a new function at the end of the file, with the
    // cursor after `path`.
    const text = `${source(server.root, rel)}\nfn __s21_probe() {\n    let _ = ${path};\n}\n`;
    server.open(rel, text);
    const labels = await server.completion(
      rel,
      position(text, `${path};`, { offset: path.length }),
    );
    show(`completion after \`${path}\` in ${rel}`, `${labels.length} items: ${labels.join(', ')}`);
    const missing = wanted.filter((label) => !labels.includes(label));
    const why = missing.length ? `missing ${missing.join(', ')}` : '';
    pass = verdict(`c. \`${path}\` in ${rel} offers ${wanted.join(', ')}`, !why, why) && pass;
  }
  return pass;
}

// d. No diagnostic that says tools/uuid/mod.rs is outside the crate.
async function diagnostics(server) {
  const found = await listDiagnostics(server, MOD);
  const bad = found.filter((d) => UNLINKED.has(d.code));
  const named = [...UNLINKED].map(([code, meaning]) => `${meaning} (${code})`).join(', ');
  return verdict(`d. none of these diagnostics for ${MOD}: ${named}`, bad.length === 0);
}

/** Opens `rel` as it is on disk, and prints and returns its diagnostics. */
async function listDiagnostics(server, rel) {
  server.open(rel, source(server.root, rel));
  const found = await server.diagnostics(rel);
  show(`diagnostics for ${rel}`, found.length ? `${found.length}` : 'none');
  for (const d of found) {
    const line = d.message.split('\n')[0];
    console.log(`    ${d.code ?? '(no code)'} at line ${d.range.start.line + 1}: ${line}`);
  }
  return found;
}

// The controls' precondition: rust-analyzer loaded the copy's workspace.
async function loaded(server) {
  const text = source(server.root, REGISTRY_TEST);
  server.open(REGISTRY_TEST, text);
  const at = position(text, '-> Registry', { offset: '-> '.length });
  const targets = await server.definition(REGISTRY_TEST, at);
  show(`definition of \`Registry\` in ${REGISTRY_TEST}`, targets);
  const file = `${CORE}registry.rs`;
  const pass =
    targets.length > 0 &&
    targets.every((t) => t.rel === file && declares(t.text, 'struct Registry'));
  return verdict(`workspace loaded: \`Registry\` in ${REGISTRY_TEST} lands in ${file}`, pass);
}

// The entry removed: mod.rs is reported unlinked, and b and c fail.
async function controlRemoved(server) {
  if (!(await loaded(server))) return false;
  const found = await listDiagnostics(server, MOD);
  const unlinked = found.some((d) => d.code === 'unlinked-file');
  verdict(`${MOD} is reported as an unlinked file`, unlinked);
  const b = await definitionCore(server);
  const c = await completion(server);
  return verdict(
    'control: with the entry removed, mod.rs is unlinked and b and c fail',
    unlinked && !b && !c,
  );
}

// The entry misspelled: a fails.
async function controlMisspelled(server) {
  if (!(await loaded(server))) return false;
  const a = await definitionList(server, MISSPELLED);
  await listDiagnostics(server, LIB); // for the log: what rust-analyzer makes of `mod uiud;`
  return verdict(`control: with the entry misspelled \`${MISSPELLED}\`, a fails`, !a);
}

/** Copies the checkout's files, tracked and untracked but not ignored, to a new temporary folder. */
function copyCheckout() {
  const args = ['ls-files', '-z', '--cached', '--others', '--exclude-standard'];
  const files = execFileSync('git', args, { cwd: repo, encoding: 'utf8' }).split('\0');
  const copy = mkdtempSync(join(tmpdir(), 'navaja-s2-1-'));
  let count = 0;
  for (const file of files.filter(Boolean)) {
    const from = join(repo, file);
    if (!existsSync(from)) continue; // deleted in the working tree
    mkdirSync(dirname(join(copy, file)), { recursive: true });
    cpSync(from, join(copy, file));
    count++;
  }
  console.log(`Copied ${count} files to ${copy}`);
  return copy;
}

/** Replaces the uuid line of the copy's register_tools! list with `line`. */
function editList(copy, line) {
  const text = source(copy, LIB);
  const entry = `    ${ID},\n`;
  const list = text.indexOf(LIST);
  const at = list < 0 ? -1 : text.indexOf(entry, list);
  if (at < 0) throw new ProbeError(`no \`${ID},\` line in the register_tools! list in ${LIB}`);
  const edited = text.slice(0, at) + line + text.slice(at + entry.length);
  writeFileSync(join(copy, LIB), edited);
  const shown = edited.slice(list, edited.indexOf('}', list) + 1).replace(/\s+/g, ' ');
  show(`${LIB} in the copy`, shown);
}

const CHECKS = {
  'definition-list': { run: (server) => definitionList(server) },
  'definition-core': { run: definitionCore },
  completion: { run: completion },
  diagnostics: { run: diagnostics },
  'control-removed': { run: controlRemoved, line: '' },
  'control-misspelled': { run: controlMisspelled, line: `    ${MISSPELLED},\n` },
};

/**
 * Says why the probe could not ask, and returns exit code 2. On a runner it
 * also sets the step's `result` output, which the job's summary shows in
 * place of FAIL: there a failed step reads as a check that failed, or a
 * control whose broken list was accepted, and this is neither.
 */
function couldNotAsk(message) {
  console.error(`The probe could not ask: ${message}`);
  if (process.env.GITHUB_ACTIONS === 'true') {
    // A workflow command's message ends at the line's end.
    const data = message.replaceAll('%', '%25').replaceAll('\r', '%0D').replaceAll('\n', '%0A');
    console.log(`::error title=S2.1 could not ask::${data}`);
  }
  if (process.env.GITHUB_OUTPUT) {
    appendFileSync(process.env.GITHUB_OUTPUT, 'result=could not ask\n');
  }
  return 2;
}

async function main() {
  const check = CHECKS[process.argv[2]];
  if (!check) {
    console.error(
      `usage: node scripts/spikes/s2-1-ra-probe.mjs <${Object.keys(CHECKS).join('|')}>`,
    );
    return couldNotAsk(`no check named ${JSON.stringify(process.argv[2] ?? '')}`);
  }
  let copy;
  let server;
  try {
    if (check.line !== undefined) {
      copy = copyCheckout();
      editList(copy, check.line);
    }
    server = new Server(copy ?? repo);
    await server.start();
    return (await check.run(server)) ? 0 : 1;
  } catch (error) {
    if (error instanceof ProbeError) return couldNotAsk(error.message);
    // A bug in the probe. Node would exit 1 on it, which means a failed check.
    console.error(error);
    return couldNotAsk(`a bug in the probe: ${error}`);
  } finally {
    await server?.stop();
    if (copy) rmSync(copy, { recursive: true, force: true });
  }
}

process.exitCode = await main();
