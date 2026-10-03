// A tiny hash router: #/, #/tool/<id>, #/settings, #/about.

export type Route =
  | { kind: 'home' }
  | { kind: 'tool'; id: string }
  | { kind: 'settings' }
  | { kind: 'about' };

const TOOL_ID = /^[a-z][a-z0-9_]*$/;

export function parseRoute(hash: string): Route {
  const path = hash.replace(/^#\/?/, '');
  if (path.startsWith('tool/')) {
    const id = path.slice('tool/'.length);
    if (TOOL_ID.test(id)) return { kind: 'tool', id };
  }
  if (path === 'settings') return { kind: 'settings' };
  if (path === 'about') return { kind: 'about' };
  return { kind: 'home' };
}

export function href(route: Route): string {
  switch (route.kind) {
    case 'tool':
      return `#/tool/${route.id}`;
    case 'settings':
      return '#/settings';
    case 'about':
      return '#/about';
    default:
      return '#/';
  }
}

class Router {
  route = $state<Route>(parseRoute(window.location.hash));

  constructor() {
    window.addEventListener('hashchange', () => {
      this.route = parseRoute(window.location.hash);
    });
  }

  go(route: Route): void {
    window.location.hash = href(route);
    this.route = route;
  }
}

export const router = new Router();
