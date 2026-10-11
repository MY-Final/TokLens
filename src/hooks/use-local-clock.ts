import { useEffect, useState } from "react";

/** Refresh rolling ranges at local minute boundaries and after focus/resume. */
export function useLocalClock(): number {
  const [nowMs, setNowMs] = useState(() => Date.now());

  useEffect(() => {
    let timer: ReturnType<typeof setTimeout>;
    const update = () => {
      clearTimeout(timer);
      const now = Date.now();
      setNowMs(now);
      timer = setTimeout(update, 60_000 - (now % 60_000));
    };
    const handleVisibility = () => {
      if (document.visibilityState === "visible") update();
    };
    update();
    window.addEventListener("focus", update);
    document.addEventListener("visibilitychange", handleVisibility);
    return () => {
      clearTimeout(timer);
      window.removeEventListener("focus", update);
      document.removeEventListener("visibilitychange", handleVisibility);
    };
  }, []);

  return nowMs;
}
