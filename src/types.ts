export type SyncState =
  | "synced"
  | "ahead"
  | "behind"
  | "diverged"
  | "no_upstream"
  | "no_remote"
  | "error";

import type { OpenTarget } from "./open-targets";

export type RepoEntry = {
  name: string;
  path: string;
  url?: string | null;
};

export type RepoConfig = {
  repos: RepoEntry[];
  open_target?: OpenTarget;
};

export type RepoStatus = {
  name: string;
  path: string;
  branch: string | null;
  dirty: boolean;
  changed_count: number;
  ahead: number;
  behind: number;
  sync_state: SyncState;
  badge: string;
  error: string | null;
  /** UI placeholder while git scan is pending (no prior data) */
  loading?: boolean;
  /** Re-scan in progress; stale data may still be shown */
  refreshing?: boolean;
};

export type ScanResult = {
  repos: RepoStatus[];
  scanned_at: string;
  summary: string;
};

export type ValidateResult = {
  ok: boolean;
  message: string;
  resolved_path: string | null;
  suggested_name: string | null;
  remote_url: string | null;
};
