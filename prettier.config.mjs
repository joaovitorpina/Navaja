// The Prettier config lives in app/, where pnpm installs Prettier and its
// plugin. It is loaded from here so that custom views in tools/<id>/ui/, which
// sit outside app/, use it too.
export { default } from './app/prettier.config.js';
