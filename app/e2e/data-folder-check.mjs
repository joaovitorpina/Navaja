// Roadmap §2, "the webview data folder holds no input text", on every PR
// (.github/workflows/ci.yml, os job, right after the end-to-end tests).
//
// The last spec, app/e2e/last/data-folder.e2e.ts, types a fresh canary into
// every text field the app has, quits the app through its `quit` command,
// and records the canary in app/e2e/output/data-folder.json. This script
// then searches every folder where the end-to-end build keeps data on this
// OS, listed with the reason for each in `locations` below.
//
// Usage, from anywhere in the repository, after `pnpm e2e`:
//   node app/e2e/data-folder-check.mjs           # no file in those folders holds the canary
//   node app/e2e/data-folder-check.mjs control   # negative control: plants the canary in each
//                                                # folder, must find every copy, then removes them
//
// Every file is read as raw bytes, binary and compressed ones included. A hit
// is:
// - the canary in UTF-8 or UTF-16LE;
// - any 12 characters of it in a row, in either encoding. Chromium's LevelDB
//   files compress blocks with Snappy, which can swap a stretch that repeats
//   earlier bytes for a reference and so split the canary;
// - either of those in the decompressed contents of a gzip or zlib file.
//
// It fails closed. These all fail the check: no record, a folder that must
// exist and doesn't, a folder that always holds files and is empty, zero
// files searched, a folder or file it can't read, and folders that are still
// changing after a minute.
import { randomBytes } from 'node:crypto';
import {
  closeSync,
  existsSync,
  lstatSync,
  mkdirSync,
  openSync,
  readdirSync,
  readFileSync,
  readSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { homedir } from 'node:os';
import { isAbsolute, join, relative, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { constants, deflateSync, gunzipSync, gzipSync, inflateSync } from 'node:zlib';

const repo = fileURLToPath(new URL('../../', import.meta.url));

/** Written by last/data-folder.e2e.ts (wdio.conf.ts, `dataFolderRecord`). */
const RECORD = join(repo, 'app', 'e2e', 'output', 'data-folder.json');

/** The end-to-end build's identifier, which names Tauri's folders. */
const E2E_CONFIG = join(repo, 'app', 'src-tauri', 'e2e.conf.json');

/** How many characters of the canary in a row count as a hit. */
const FRAGMENT = 12;

/** Files are read in chunks of this size, so a large one never fills memory. */
const CHUNK = 8 * 2 ** 20;

/** Compressed files up to this size are also decompressed and searched. */
const MAX_DECOMPRESS = 256 * 2 ** 20;

/** The folders must stop changing for this long before the search... */
const QUIET_MS = 3_000;
/** ...and get this long to do so, after the app has quit. */
const SETTLE_MS = 60_000;

/** A file that is still locked (Windows) is tried again this often. */
const READ_TRIES = 10;

let failed = false;

function fail(message) {
  console.log(`::error::data folder: ${message}`);
  failed = true;
}

function sleep(ms) {
  Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, ms);
}

function readRecord() {
  if (!existsSync(RECORD)) {
    fail(
      `no record at ${RECORD}; run the end-to-end suite first (pnpm e2e), whose last spec writes it`,
    );
    return null;
  }
  let record;
  try {
    record = JSON.parse(readFileSync(RECORD, 'utf8'));
  } catch (error) {
    fail(`cannot read the record ${RECORD}: ${error.message}`);
    return null;
  }
  // An empty or short canary would match far too much, or nothing at all.
  if (typeof record.canary !== 'string' || !/^[a-z0-9]{32}$/.test(record.canary)) {
    fail(`the record's canary is not 32 lower-case letters and digits: ${RECORD}`);
    return null;
  }
  if (record.platform !== process.platform) {
    fail(`the record comes from ${record.platform}, not ${process.platform}`);
    return null;
  }
  if (typeof record.appDir !== 'string' || !isAbsolute(record.appDir)) {
    fail('the record names no absolute NAVAJA_APP_DIR');
    return null;
  }
  if (!Array.isArray(record.fields) || record.fields.length === 0) {
    fail('the record names no field the canary was typed into');
    return null;
  }
  return record;
}

/**
 * The folders to search, each with why and whether it must exist. `full`
 * marks one that always holds files once the app has run, so an empty one
 * means the search is looking in the wrong place.
 *
 * The values come from the environment the app started with (the record's
 * `env`), resolved the way the `dirs` crate does, which Tauri uses.
 */
function locations(record, identifier) {
  const env = (name) => record.env?.[name] || undefined;
  const home = env('HOME') ?? homedir();
  // dirs-sys: an XDG variable counts only when it holds an absolute path.
  const xdg = (name, fallback) => {
    const value = env(name);
    return value && isAbsolute(value) ? value : join(home, fallback);
  };

  const list = [
    {
      path: record.appDir,
      why: "NAVAJA_APP_DIR, the app's own folder: settings, logs and crash files. wdio.conf.ts makes a fresh one for each run",
      required: true,
      full: true,
    },
  ];

  switch (process.platform) {
    case 'win32': {
      const local = env('LOCALAPPDATA');
      const roaming = env('APPDATA');
      if (!local || !roaming) {
        fail('LOCALAPPDATA or APPDATA was not set for the app');
        return list;
      }
      const appLocal = join(local, identifier);
      const userData = env('WEBVIEW2_USER_DATA_FOLDER') ?? appLocal;
      list.push(
        {
          path: appLocal,
          why: "Tauri's app local data and app cache folder (both under %LOCALAPPDATA% on Windows). Tauri creates it and passes it to WebView2 as the user data folder, since on Windows and Linux it always sets the webview's data directory (tauri's manager/webview.rs)",
          required: true,
        },
        {
          path: join(userData, 'EBWebView'),
          why: "WebView2's folder inside the user data folder: its profile, caches and crash reports. WebView2 creates it on start. WEBVIEW2_USER_DATA_FOLDER, when set, replaces the user data folder",
          required: true,
          full: true,
        },
        {
          path: join(roaming, identifier),
          why: "Tauri's app data and app config folder (under %APPDATA% on Windows). Nothing should write there",
          required: false,
        },
      );
      break;
    }
    case 'linux': {
      const data = xdg('XDG_DATA_HOME', join('.local', 'share'));
      const cache = xdg('XDG_CACHE_HOME', '.cache');
      const config = xdg('XDG_CONFIG_HOME', '.config');
      list.push(
        {
          path: join(data, identifier),
          why: "Tauri's app data and app local data folder ($XDG_DATA_HOME). Tauri creates it and sets it as the webview's data directory; wry makes it the base data and cache folder of a WebKitGTK website data manager, cookies included. The webview is incognito, so wry gives it an ephemeral context instead, and this folder should stay empty",
          required: true,
        },
        {
          path: join(cache, identifier),
          why: "Tauri's app cache folder ($XDG_CACHE_HOME)",
          required: false,
        },
        {
          path: join(config, identifier),
          why: "Tauri's app config folder ($XDG_CONFIG_HOME)",
          required: false,
        },
      );
      for (const name of ['navaja', 'webkitgtk']) {
        list.push(
          {
            path: join(data, name),
            why: `WebKitGTK's default data folder for a context that names none, for ${name}`,
            required: false,
          },
          {
            path: join(cache, name),
            why: `WebKitGTK's default cache folder for a context that names none, for ${name}`,
            required: false,
          },
        );
      }
      break;
    }
    case 'darwin': {
      const library = join(home, 'Library');
      list.push({
        path: join(library, 'Application Support', identifier),
        why: "Tauri's app data, app local data and app config folder",
        required: false,
      });
      // WebKit and CFNetwork name their folders after the bundle identifier,
      // or after the process when there is no bundle, as for the bare
      // target/debug/navaja that the suite runs.
      for (const name of [identifier, 'navaja']) {
        list.push(
          {
            path: join(library, 'WebKit', name),
            why: `WebKit's website data folder, for ${name}`,
            required: false,
          },
          {
            path: join(library, 'Caches', name),
            why: `WebKit's and NSURLCache's cache folder, for ${name}; for the identifier, Tauri's app cache folder too`,
            required: false,
          },
          {
            path: join(library, 'HTTPStorages', name),
            why: `CFNetwork's cookie and HSTS storage, for ${name}`,
            required: false,
          },
        );
      }
      break;
    }
    default:
      fail(`no folder list for ${process.platform}`);
  }
  return list;
}

/** Whether `inner` is `outer` or a path inside it. */
function within(inner, outer) {
  const path = relative(outer, inner);
  return path === '' || (!path.startsWith('..') && !isAbsolute(path));
}

/**
 * Every regular file under `root`, with its size and modification time.
 * Symbolic links are listed apart and never followed: they lead outside.
 */
function walk(root, files, links) {
  let entries;
  try {
    entries = readdirSync(root, { withFileTypes: true });
  } catch (error) {
    fail(`cannot list ${root}: ${error.code ?? error.message}`);
    return;
  }
  for (const entry of entries) {
    const path = join(root, entry.name);
    if (entry.isSymbolicLink()) {
      links.push(path);
    } else if (entry.isDirectory()) {
      walk(path, files, links);
    } else if (entry.isFile()) {
      try {
        const info = lstatSync(path);
        files.push({ path, size: info.size, mtime: info.mtimeMs });
      } catch (error) {
        // Gone since the listing: a later snapshot shows whether it settled.
        if (error.code !== 'ENOENT') fail(`cannot stat ${path}: ${error.code ?? error.message}`);
      }
    }
  }
}

/** The folders to walk: those that exist and are not inside another one. */
function roots(list) {
  const present = [...new Set(list.map((location) => location.path))].filter((path) =>
    existsSync(path),
  );
  return present.filter((path) => !present.some((other) => other !== path && within(path, other)));
}

function snapshot(list) {
  const files = [];
  const links = [];
  for (const root of roots(list)) walk(root, files, links);
  return { files, links };
}

const fingerprint = (files) =>
  files
    .map((file) => `${file.path}\0${file.size}\0${file.mtime}`)
    .sort()
    .join('\n');

/**
 * Waits until no file in the folders changes for QUIET_MS. WebView2 and
 * WebKit write from their own processes, which may outlive the app by a
 * moment.
 */
function settle(list) {
  const deadline = Date.now() + SETTLE_MS;
  let previous = fingerprint(snapshot(list).files);
  for (;;) {
    sleep(QUIET_MS);
    const current = snapshot(list);
    const print = fingerprint(current.files);
    if (print === previous) return current;
    if (Date.now() > deadline) {
      fail(`the folders were still changing ${SETTLE_MS / 1000} s after the app quit`);
      return current;
    }
    previous = print;
  }
}

/** The byte strings that count as a hit, each with what it is. */
function needles(canary) {
  const list = [];
  for (const [encoding, label] of [
    ['utf8', 'UTF-8'],
    ['utf16le', 'UTF-16LE'],
  ]) {
    list.push({ whole: true, label, bytes: Buffer.from(canary, encoding) });
    for (let at = 0; at + FRAGMENT <= canary.length; at++) {
      list.push({
        whole: false,
        label,
        bytes: Buffer.from(canary.slice(at, at + FRAGMENT), encoding),
      });
    }
  }
  return list;
}

const WHOLE = 'the canary';
const PART = `${FRAGMENT} characters of it`;

/** What `needles` matched in `bytes`: the canary or part of it, per encoding. */
function matches(bytes, list, found) {
  for (const needle of list) {
    const form = `${needle.whole ? WHOLE : PART} in ${needle.label}`;
    if (!found.has(form) && bytes.includes(needle.bytes)) found.add(form);
  }
}

/** Drops "part of it" wherever the whole canary was found the same way. */
function plainest(found) {
  return new Set(
    [...found].filter((form) => !(form.startsWith(PART) && found.has(form.replace(PART, WHOLE)))),
  );
}

/** The decoder for a file that starts like a gzip or zlib stream, if any. */
function decoder(head) {
  if (head.length >= 3 && head[0] === 0x1f && head[1] === 0x8b && head[2] === 0x08) {
    return {
      name: 'gzip',
      decode: (bytes) => gunzipSync(bytes, { finishFlush: constants.Z_SYNC_FLUSH }),
    };
  }
  // zlib: deflate with a 32 KiB window, and a header checksum.
  if (head.length >= 2 && head[0] === 0x78 && (head[0] * 256 + head[1]) % 31 === 0) {
    return {
      name: 'zlib',
      decode: (bytes) => inflateSync(bytes, { finishFlush: constants.Z_SYNC_FLUSH }),
    };
  }
  return null;
}

function readChunks(path, onChunk) {
  const fd = openSync(path, 'r');
  try {
    const buffer = Buffer.allocUnsafe(CHUNK);
    for (let position = 0; ;) {
      const read = readSync(fd, buffer, 0, CHUNK, position);
      if (read === 0) return;
      onChunk(buffer.subarray(0, read), position);
      position += read;
    }
  } finally {
    closeSync(fd);
  }
}

/**
 * Searches one file. Returns what it found ("the canary in UTF-8", "in its
 * gzip contents: ..."), or null when the file could not be read.
 */
function searchFile(file, list) {
  const overlap = Math.max(...list.map((needle) => needle.bytes.length)) - 1;
  for (let attempt = 1; ; attempt++) {
    try {
      const found = new Set();
      let tail = Buffer.alloc(0);
      let head = null;
      readChunks(file.path, (chunk, position) => {
        if (position === 0) head = Buffer.from(chunk.subarray(0, 4));
        // The end of the previous chunk, so a hit across the boundary counts.
        const window = Buffer.concat([tail, chunk]);
        matches(window, list, found);
        tail = Buffer.from(window.subarray(Math.max(0, window.length - overlap)));
      });
      const codec = head && file.size <= MAX_DECOMPRESS ? decoder(head) : null;
      if (codec) {
        let decoded = null;
        try {
          decoded = codec.decode(readFileSync(file.path));
        } catch {
          // Not a stream after all, only bytes that start like one.
        }
        if (decoded) {
          const inner = new Set();
          matches(decoded, list, inner);
          for (const form of inner) found.add(`${form}, in its ${codec.name} contents`);
        }
      }
      return plainest(found);
    } catch (error) {
      // WebView2 may still hold a file for a moment after the app quits.
      const locked = error.code === 'EBUSY' || error.code === 'EPERM' || error.code === 'EACCES';
      if (locked && attempt < READ_TRIES) {
        sleep(1_000);
        continue;
      }
      fail(`cannot read ${file.path}: ${error.code ?? error.message}`);
      return null;
    }
  }
}

/** Searches every file under the folders; returns the files with a hit. */
function search(list, files, canary) {
  const needleList = needles(canary);
  const hits = [];
  for (const file of files) {
    const found = searchFile(file, needleList);
    if (found && found.size > 0) hits.push({ path: file.path, forms: [...found] });
  }
  return hits;
}

function report(list, files) {
  for (const location of list) {
    const exists = existsSync(location.path);
    const inside = files.filter((file) => within(file.path, location.path));
    const bytes = inside.reduce((sum, file) => sum + file.size, 0);
    const state = exists ? `${inside.length} files, ${bytes} bytes` : 'absent';
    console.log(
      `- ${location.path} (${location.required ? 'must exist' : 'if present'}): ${state}`,
    );
    console.log(`  ${location.why}.`);
    if (location.required && !exists) fail(`${location.path} must exist and does not`);
    if (location.full && exists && inside.length === 0) {
      fail(`${location.path} always holds files once the app has run, and is empty`);
    }
  }
}

function check(record, list) {
  console.log(`Canary: ${record.canary}, typed into: ${record.fields.join('; ')}.`);
  console.log(`The app quit at ${record.quitAt}. Searching:`);
  const { files, links } = settle(list);
  report(list, files);
  for (const link of links) console.log(`Not followed, a symbolic link: ${link}`);
  if (files.length === 0) fail('zero files searched');
  const hits = search(list, files, record.canary);
  for (const hit of hits) {
    fail(`${hit.path} holds ${hit.forms.join('; ')}`);
  }
  if (!failed) {
    console.log(
      `No input text in ${files.length} files: none holds the canary or ${FRAGMENT} characters of it.`,
    );
  }
}

/** What the control plants in each folder, and the hit each must give. */
const SAMPLES = [
  { name: 'utf-8.txt', form: `${WHOLE} in UTF-8`, bytes: (c) => Buffer.from(`typed: ${c}\n`) },
  {
    name: 'utf-16le.bin',
    form: `${WHOLE} in UTF-16LE`,
    bytes: (c) => Buffer.from(`typed: ${c}`, 'utf16le'),
  },
  // Split as a Snappy reference would split it: only the pieces remain.
  {
    name: 'split.bin',
    form: `${PART} in UTF-8`,
    bytes: (c) =>
      Buffer.concat([
        Buffer.from(c.slice(0, 16)),
        Buffer.from([0x00, 0xff]),
        Buffer.from(c.slice(16)),
      ]),
  },
  {
    name: 'text.gz',
    form: `${WHOLE} in UTF-8, in its gzip contents`,
    bytes: (c) => gzipSync(Buffer.from(c)),
  },
  {
    name: 'text.zlib',
    form: `${WHOLE} in UTF-8, in its zlib contents`,
    bytes: (c) => deflateSync(Buffer.from(c)),
  },
];

function control(record, list) {
  const present = [...new Set(list.map((location) => location.path))].filter((path) =>
    existsSync(path),
  );
  if (present.length === 0) {
    fail('none of the folders exists, so there is nowhere to plant the canary');
    return;
  }
  const folders = present.map((path) =>
    join(path, `navaja-control-${randomBytes(6).toString('hex')}`),
  );
  try {
    for (const folder of folders) {
      mkdirSync(folder);
      for (const sample of SAMPLES)
        writeFileSync(join(folder, sample.name), sample.bytes(record.canary));
    }
    const { files } = snapshot(list);
    const hits = search(list, files, record.canary);
    for (const folder of folders) {
      for (const sample of SAMPLES) {
        const path = join(folder, sample.name);
        const hit = hits.find((candidate) => candidate.path === path);
        if (!hit) fail(`the control missed ${path}`);
        else if (!hit.forms.includes(sample.form)) {
          fail(`the control found ${hit.forms.join('; ')} in ${path}, not ${sample.form}`);
        }
      }
    }
    for (const hit of hits) {
      if (!folders.some((folder) => hit.path.startsWith(folder + sep))) {
        fail(`${hit.path} holds ${hit.forms.join('; ')}, and the control did not plant it`);
      }
    }
    console.log(`Planted ${SAMPLES.length} files in each of ${folders.length} folders:`);
    for (const folder of folders) console.log(`- ${folder}`);
    console.log(`The search found ${hits.length} of ${SAMPLES.length * folders.length}.`);
  } finally {
    for (const folder of folders) rmSync(folder, { recursive: true, force: true });
  }
  const left = search(list, snapshot(list).files, record.canary);
  if (left.length > 0 || folders.some((folder) => existsSync(folder))) {
    fail('the planted files are still there after their removal');
  } else if (!failed) {
    console.log('Removed them again; the folders hold no canary.');
  }
}

const mode = process.argv[2] ?? 'check';
if (mode !== 'check' && mode !== 'control') {
  console.error('usage: node app/e2e/data-folder-check.mjs [check|control]');
  process.exit(2);
}
const record = readRecord();
if (record) {
  const identifier = JSON.parse(readFileSync(E2E_CONFIG, 'utf8')).identifier;
  const list = locations(record, identifier);
  if (mode === 'check') check(record, list);
  else control(record, list);
}
process.exit(failed ? 1 : 0);
