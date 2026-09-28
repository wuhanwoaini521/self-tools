/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly VITE_TAURI_E2E?: string;
}

interface Window {
  /** Test adapters are installed only by the dedicated E2E bundle. */
  __DEVTOOLBOX_E2E__?: boolean;
  /** Present only in the dedicated e2e bundle to replace the native file chooser. */
  __DEVTOOLBOX_E2E_OPEN_DOCUMENT__?: { path: string; content: string } | null;
  __DEVTOOLBOX_E2E_SAVED_DOCUMENT__?: { path: string; content: string };
  __DEVTOOLBOX_E2E_SET_EDITOR_LINE__?: (text: string) => boolean;
  __DEVTOOLBOX_E2E_SERVER_FIXTURE__?: unknown;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
