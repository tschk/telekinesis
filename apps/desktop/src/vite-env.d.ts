/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly VITE_TK_CLOUD_API?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
