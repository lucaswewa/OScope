// What a menu holds (ADR-0042): the same for one under a button and one
// where the pointer was.
import type { IconName } from "./icons";

/** One thing a menu offers to do. */
export interface MenuEntry {
  /** What `select` says was chosen. */
  id: string;
  label: string;
  icon?: IconName;
  disabled?: boolean;
  /** For what deletes or stops: drawn in the danger colour. */
  danger?: boolean;
  /** A line above it: the start of another group. */
  separated?: boolean;
}
