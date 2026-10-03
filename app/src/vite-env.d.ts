/// <reference types="vite/client" />

interface ImportMetaEnv {
  /** "1" in end-to-end test builds only. */
  readonly VITE_NAVAJA_E2E?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
