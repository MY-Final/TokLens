import type { ClientKey } from "@/lib/usage-types";

export type ReportFormat = "csv" | "json" | "markdown";
export type ExportArgs = {
  directory: string;
  format: ReportFormat;
  fromMs: number | null;
  toMs: number;
  client: ClientKey | null;
  includeEstimated: boolean;
};
export type ExportResult = {
  path: string;
  rowCount: number;
  untimedRequests: number;
  excludedEstimatedRequests: number;
};
