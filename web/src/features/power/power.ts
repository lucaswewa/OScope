// Container restart: its response and how the app recognizes the new process.
import type { Output } from "@/api/actions";

/** The actions, the service's first. Each is confirmed by typing its name. */
export const ACTIONS = ["restart"] as const;

export type PowerAction = (typeof ACTIONS)[number];

/** What an action did, or in a dry run would have done. */
export type Powered = Output<"/api/v1/power/restart">;

/** The command an action runs, as a line: none for a restart, which the server does itself. */
export function commandOf(powered: Powered): string | undefined {
  return powered.command?.join(" ");
}

/**
 * Whether the server answering has started since a restart was asked for:
 * up for less time than has passed since. Until then, the one that was asked
 * may still be stopping.
 */
export function isBack(uptimeSeconds: number, sinceMs: number): boolean {
  return uptimeSeconds * 1000 < sinceMs;
}
