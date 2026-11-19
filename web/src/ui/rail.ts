// One of the rail's tabs (W2.1), as the app gives it to `Rail`.
import type { IconName } from "./icons";

export interface RailItem {
  /** Where it goes: a hash route. */
  href: string;
  label: string;
  icon: IconName;
  /** Whether it is the tab shown. */
  current: boolean;
}
