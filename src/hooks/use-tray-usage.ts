import { startOfDay } from "date-fns";
import { useEffect } from "react";
import { refreshTrayUsage } from "@/lib/api";

/** Keep native today's usage independent of page filters and window visibility. */
export function useTrayUsage(nowMs: number, refreshToken: number): void {
  useEffect(() => {
    // Register new boundaries immediately: native generations reject older results,
    // including an in-flight query when the date changes or statistics are cleared.
    void refreshTrayUsage(
      startOfDay(nowMs).getTime(),
      Math.floor(nowMs / 60_000) * 60_000 + 60_000,
    ).catch(() => {
      // The native summary presents a sanitized unavailable state.
    });
  }, [nowMs, refreshToken]);
}
