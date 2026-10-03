// The import allowlist for custom views in tools/<id>/ui/ (docs/architecture.md §4).
// A view reaches the shell only through $lib/view-kit, so the shell can change
// behind that module without touching any tool.
import path from 'node:path';

/** Packages a view's own tests may import; S2.2 made exactly these resolvable from tools/. */
const TEST_PACKAGES = new Set(['vitest', '@testing-library/svelte', '@tauri-apps/api/mocks']);

/**
 * Svelte's public entry points for code in the browser. Not svelte/internal
 * (private), svelte/compiler or svelte/server, which have no place in a
 * webview. A test compares this list with the exports of the installed Svelte.
 */
export const SVELTE_ENTRIES = new Set([
  'svelte',
  'svelte/action',
  'svelte/animate',
  'svelte/attachments',
  'svelte/easing',
  'svelte/elements',
  'svelte/events',
  'svelte/legacy',
  'svelte/motion',
  'svelte/reactivity',
  'svelte/reactivity/window',
  'svelte/store',
  'svelte/transition',
]);

/**
 * Vitest's calls that take a module specifier, on `vi` and its alias `vitest`.
 * Vitest resolves it through the same aliases as an import, and importActual
 * loads the module itself.
 */
const VITEST_MODULE_CALLS = new Set([
  'mock',
  'doMock',
  'unmock',
  'doUnmock',
  'importActual',
  'importMock',
]);

const TEST_FILE = /\.(test|spec)\.ts$/;
const BINDING = /^\$bindings(\/[A-Za-z0-9_-]+)+$/;

const ALLOWED =
  'relative paths inside its own tools/<id>/ui/ folder, svelte and its browser subpaths ' +
  '(not svelte/internal, svelte/compiler or svelte/server), $lib/view-kit and $bindings/<name>';
const ALLOWED_IN_TESTS =
  `${ALLOWED}; its *.test.ts and *.spec.ts files also vitest, ` +
  '@testing-library/svelte and @tauri-apps/api/mocks';
const SEE = 'See docs/architecture.md §4.';

/**
 * `filename` with forward slashes, so that path.posix works on it on every
 * OS, and normalized as path.posix.join normalizes the paths it is compared
 * with: a UNC name (\\host\share\…) would otherwise keep a leading `//` that
 * the joined paths lose. A \\?\ prefix goes too. A name with no folder
 * (`<input>`) gets one, so `..` still leaves it.
 * @param {string} filename
 */
function posix(filename) {
  const file = path.posix.normalize(filename.replaceAll('\\', '/').replace(/^\/\/\?\//, ''));
  return file.startsWith('/') || /^[A-Za-z]:\//.test(file) ? file : `/unnamed/${file}`;
}

/**
 * The tools/<id>/ui folder that holds `file`, ending in a slash. Outside one,
 * the file's own folder, so a relative import still may not climb out.
 * @param {string} file
 */
function uiRoot(file) {
  const root = /^(.*\/tools\/[^/]+\/ui)\//.exec(file)?.[1] ?? path.posix.dirname(file);
  return root.endsWith('/') ? root : `${root}/`;
}

/** @param {string} specifier */
function isRelative(specifier) {
  return (
    specifier === '.' ||
    specifier === '..' ||
    specifier.startsWith('./') ||
    specifier.startsWith('../')
  );
}

/**
 * Whether a relative specifier stays inside the view's ui folder. It is
 * checked with and without its ?query or #hash, since Vite strips them. Back
 * slashes and percent escapes are refused: some resolvers read them as
 * separators or decode them, so this check could disagree with the build.
 * @param {string} file
 * @param {string} specifier
 */
function staysInside(file, specifier) {
  if (/[\\%]/.test(specifier)) return false;
  const root = uiRoot(file);
  const from = path.posix.dirname(file);
  const bare = specifier.replace(/[?#].*$/s, '');
  return [specifier, bare].every((s) => `${path.posix.join(from, s)}/`.startsWith(root));
}

/**
 * @param {string} file
 * @param {string} specifier
 */
function isAllowed(file, specifier) {
  if (isRelative(specifier)) return staysInside(file, specifier);
  if (SVELTE_ENTRIES.has(specifier)) return true;
  if (specifier === '$lib/view-kit') return true;
  if (BINDING.test(specifier)) return true;
  return TEST_FILE.test(file) && TEST_PACKAGES.has(specifier);
}

/**
 * The string a specifier node spells, or undefined when it is computed.
 * @param {any} node
 * @returns {string | undefined}
 */
function literal(node) {
  if (node?.type === 'Literal' && typeof node.value === 'string') return node.value;
  if (node?.type === 'TemplateLiteral' && node.expressions.length === 0) {
    return node.quasis[0]?.value.cooked ?? undefined;
  }
  return undefined;
}

/**
 * Whether `node` is `import.meta`.
 * @param {any} node
 */
function isImportMeta(node) {
  return (
    node?.type === 'MetaProperty' && node.meta.name === 'import' && node.property.name === 'meta'
  );
}

/**
 * The property name a member expression reads, when it is spelled out.
 * @param {any} node
 */
function propertyName(node) {
  return node.computed ? literal(node.property) : node.property.name;
}

/**
 * Whether `callee` is `vi.mock`, `vitest.importActual` or another call that
 * takes a module specifier.
 * @param {any} callee
 */
function isVitestModuleCall(callee) {
  if (callee.type !== 'MemberExpression' || callee.object.type !== 'Identifier') return false;
  const name = propertyName(callee);
  return (
    (callee.object.name === 'vi' || callee.object.name === 'vitest') &&
    typeof name === 'string' &&
    VITEST_MODULE_CALLS.has(name)
  );
}

/** @type {import('eslint').Rule.RuleModule} */
const rule = {
  meta: {
    type: 'problem',
    docs: {
      description: 'Restrict what a custom view in tools/<id>/ui/ may import',
    },
    schema: [],
    messages: {
      notAllowed:
        "'{{source}}' is not allowed here. A custom view imports only {{allowed}}. {{see}}",
      nonLiteral:
        'A custom view imports only string literals, so this check can read them. ' +
        'It may import {{allowed}}. {{see}}',
      glob:
        'A custom view may not use import.meta.glob: it loads files this check cannot see. ' +
        'It may import {{allowed}}. {{see}}',
    },
  },
  create(context) {
    const file = posix(context.filename);
    const isTest = TEST_FILE.test(file);
    const allowed = isTest ? ALLOWED_IN_TESTS : ALLOWED;

    /**
     * Reports `node` unless it spells an allowed specifier.
     * @param {any} node
     */
    function check(node) {
      const source = literal(node);
      if (source === undefined) {
        context.report({ node, messageId: 'nonLiteral', data: { allowed, see: SEE } });
      } else if (!isAllowed(file, source)) {
        context.report({ node, messageId: 'notAllowed', data: { source, allowed, see: SEE } });
      }
    }

    return {
      ImportDeclaration(node) {
        check(node.source);
      },
      ExportNamedDeclaration(node) {
        if (node.source) check(node.source);
      },
      ExportAllDeclaration(node) {
        check(node.source);
      },
      ImportExpression(node) {
        check(node.source);
      },
      CallExpression(node) {
        if (node.callee.type === 'Identifier' && node.callee.name === 'require') {
          check(node.arguments[0] ?? node);
        } else if (isTest && isVitestModuleCall(node.callee)) {
          // vi.mock(import('…')) is checked as an import already.
          const [target] = node.arguments;
          if (target?.type !== 'ImportExpression') check(target ?? node);
        }
      },
      // Vite bundles the file behind `new URL(x, import.meta.url)`, as an
      // asset or a worker. It resolves a bare x through the aliases too
      // ($lib/ipc.ts, @tools/…), so x faces the same check as an import.
      // An absolute URL ignores the base and needs no import.meta.url.
      NewExpression(node) {
        const [target, base] = node.arguments;
        if (node.callee.type !== 'Identifier' || node.callee.name !== 'URL') return;
        if (base?.type !== 'MemberExpression' || !isImportMeta(base.object)) return;
        if (propertyName(base) === 'url') check(target);
      },
      MemberExpression(node) {
        const name = propertyName(node);
        if (isImportMeta(node.object) && typeof name === 'string' && name.startsWith('glob')) {
          context.report({ node, messageId: 'glob', data: { allowed, see: SEE } });
        }
      },
      // TypeScript's own forms: `import x = require('…')` and `typeof import('…')`.
      /** @param {any} node */
      TSExternalModuleReference(node) {
        check(node.expression);
      },
      /** @param {any} node */
      TSImportType(node) {
        check(node.source);
      },
    };
  },
};

export default rule;
