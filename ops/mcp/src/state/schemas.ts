import { z } from "zod";

export const SessionSchema = z.object({
  id: z.string().uuid(),
  repo: z.enum(["icn", "homelab-inventory"]),
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
