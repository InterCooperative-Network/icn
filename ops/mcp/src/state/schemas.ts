import { z } from "zod";

export const SessionSchema = z.object({
  id: z.string().uuid(),
  // Free-form: the runtime stores whatever repo name a launcher registers
  // (session-runtime.ts types it as `string`), and historical rows may carry
  // retired identifiers. A closed enum here would reject real history while
  // teaching readers a stale repo set (icn#2809).
  repo: z.string(),
  worktree: z.string().nullable(),
  task_description: z.string().nullable(),
  started_at: z.string(),
  last_heartbeat: z.string(),
});

export const FileClaim = z.object({
  file_path: z.string(),
  session_id: z.string().uuid(),
  claimed_at: z.string(),
});

export const HealthCacheEntry = z.object({
  key: z.string(),
  value: z.unknown(),
  polled_at: z.string(),
});

export const DecisionIndex = z.object({
  id: z.string(),
  title: z.string(),
  tags: z.string().nullable(),
  file_path: z.string(),
  created_at: z.string().nullable(),
});

export type Session = z.infer<typeof SessionSchema>;
export type FileClaim = z.infer<typeof FileClaim>;
export type HealthCacheEntry = z.infer<typeof HealthCacheEntry>;
export type DecisionIndex = z.infer<typeof DecisionIndex>;
