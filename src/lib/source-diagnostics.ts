/** Display and explicit export of source metadata; never include local paths or identifiers. */
import type { ClientSourceStatus } from "@/lib/usage-types";

export function sourceStatusLabel(source: ClientSourceStatus): string {
  const scan = source.lastResult?.report;
  if (source.lastResult?.error) return "读取或同步失败";
  if (source.sourcePaths.some((path) => path.state === "inaccessible")) return "读取失败";
  if (
    source.sourcePaths.length === 0 ||
    source.sourcePaths.every((path) => path.state === "missing")
  )
    return "来源不存在";
  if (scan && scan.filesFailed + scan.directoriesFailed > 0) return "部分读取失败";
  if ((scan?.unparsedLines ?? 0) + (scan?.filesWithoutIdentity ?? 0) + source.unparsedRecords > 0)
    return "部分解析";
  if (scan?.rootMissing) return "未发现会话文件";
  if (source.sessions > 0 && source.sessionsWithoutUsage === source.sessions)
    return "有会话 · Token 未知";
  return source.lastCheckedMs === null ? "路径可访问 · 待检查" : "已检查";
}

export function sourceDiagnosticSummary(
  source: ClientSourceStatus,
  appVersion: string | null,
): string {
  const scan = source.lastResult?.report;
  return JSON.stringify(
    {
      format: "toklens.source-diagnostics.v1",
      appVersion,
      client: source.client,
      state: sourceStatusLabel(source),
      sourceLocations: source.sourcePaths.map((path, index) => ({
        location: index + 1,
        state: path.state,
      })),
      lastCheckedMs: source.lastCheckedMs,
      lastSuccessfulScanMs: source.lastScanMs,
      lastRecordedRequestMs: source.lastRequestMs,
      sessions: source.sessions,
      sessionsWithoutUsage: source.sessionsWithoutUsage,
      requests: source.messages,
      filesTracked: source.filesTracked,
      unparsedRecords: source.unparsedRecords,
      lastScan: scan
        ? {
            rootMissing: scan.rootMissing,
            filesSeen: scan.filesSeen,
            filesParsed: scan.filesParsed,
            filesSkippedUnchanged: scan.filesSkippedUnchanged,
            filesWithoutIdentity: scan.filesWithoutIdentity,
            filesFailed: scan.filesFailed,
            directoriesFailed: scan.directoriesFailed,
            unparsedLines: scan.unparsedLines,
          }
        : null,
      syncFailed: Boolean(source.lastResult?.error),
    },
    null,
    2,
  );
}
