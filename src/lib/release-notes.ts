/** Convert release metadata to inert, readable text without loading linked resources. */
export function releaseNotesText(notes: string | null): string {
  return (notes ?? "")
    .replace(/!\[[^\]]*\]\([^)]*\)/g, "")
    .replace(/\[([^\]]+)\]\([^)]*\)/g, "$1")
    .replace(/<[^>]*>/g, "")
    .split(/\r?\n/)
    .map((line) =>
      line
        .replace(/^\s*(?:#{1,6}\s+|[-*+]\s+|>\s*|\d+[.)]\s+)/, "")
        .replace(/[*_`]/g, "")
        .trim(),
    )
    .filter((line) => line && !/^[-=]{3,}$/.test(line))
    .join("\n");
}

export function releaseNotesSummary(notes: string | null): string[] {
  const lines = (notes ?? "").split(/\r?\n/);
  const changes = lines.filter((line) => /^\s*(?:[-*+]\s+|\d+[.)]\s+)/.test(line));
  const content = changes.length ? changes : lines.filter((line) => !/^\s*#{1,6}\s/.test(line));
  return releaseNotesText(content.join("\n")).split("\n").filter(Boolean).slice(0, 3);
}
