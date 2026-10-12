import { createContext, useContext } from "react";
import { formatTokenDisplay } from "@/lib/format";
import type { TokenDisplay } from "@/lib/settings-types";

export const TokenDisplayContext = createContext<TokenDisplay>("auto");

export function useTokenFormat() {
  const display = useContext(TokenDisplayContext);
  return {
    formatTokens: (value: number) => formatTokenDisplay(value, display),
    formatTokensOrDash: (value: number | null) =>
      value === null ? "—" : formatTokenDisplay(value, display),
  };
}
