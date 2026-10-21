/**
 * 发布说明来自远程仓库，属于不可信文本：只解析成结构化块，不渲染原始 HTML，
 * 不加载远程图片与链接目标。
 */

export type ReleaseInline =
  | { kind: "text"; text: string }
  | { kind: "code"; text: string }
  | { kind: "strong"; text: string }
  | { kind: "emphasis"; text: string };

export type ReleaseBlock =
  | { kind: "heading"; level: number; text: string }
  | { kind: "paragraph"; text: string }
  | { kind: "list"; ordered: boolean; items: string[] }
  | { kind: "code"; text: string };

const HEADING_PATTERN = /^(#{1,6})\s+(.*)$/;
const LIST_PATTERN = /^\s*(?:[-*+]|\d+[.)])\s+(.*)$/;
const QUOTE_PATTERN = /^\s*>\s?(.*)$/;
const RULE_PATTERN = /^\s*(?:[-*_][ \t]*){3,}$/;
const FENCE_PATTERN = /^\s*(?:```|~~~)/;
const IMAGE_PATTERN = /!\[[^\]]*\]\([^)]*\)/g;
const HTML_PATTERN = /<[^>]*>/g;
const INLINE_PATTERN = /`([^`]+)`|\*\*([^*\n]+)\*\*|\*([^*\n]+)\*|\[([^\]]*)\]\([^)]*\)/g;

/** 去掉行内标记与 HTML，保留纯文本。 */
function releaseNotesPlainText(text: string): string {
  return text
    .replace(IMAGE_PATTERN, "")
    .replace(HTML_PATTERN, "")
    .replace(INLINE_PATTERN, (_match, code, strong, emphasis, link) => {
      return code ?? strong ?? emphasis ?? link ?? "";
    })
    .trim();
}

/** 解析行内标记：代码、加粗、强调与链接文字。 */
export function parseReleaseInline(text: string): ReleaseInline[] {
  const source = text.replace(IMAGE_PATTERN, "").replace(HTML_PATTERN, "");
  const parts: ReleaseInline[] = [];
  let cursor = 0;
  for (const match of source.matchAll(INLINE_PATTERN)) {
    const start = match.index ?? 0;
    if (start > cursor) parts.push({ kind: "text", text: source.slice(cursor, start) });
    const [raw, code, strong, emphasis, link] = match;
    if (code !== undefined) parts.push({ kind: "code", text: code });
    else if (strong !== undefined) parts.push({ kind: "strong", text: strong });
    else if (emphasis !== undefined) parts.push({ kind: "emphasis", text: emphasis });
    else if (link !== undefined) parts.push({ kind: "text", text: link });
    cursor = start + raw.length;
  }
  if (cursor < source.length) parts.push({ kind: "text", text: source.slice(cursor) });
  return parts;
}

/**
 * 解析发布说明为标题、段落、列表与代码块。
 * 未知结构按普通段落处理，图片与 HTML 标签直接丢弃。
 */
export function parseReleaseNotes(notes: string | null): ReleaseBlock[] {
  const blocks: ReleaseBlock[] = [];
  let paragraph: string[] = [];
  let list: { ordered: boolean; items: string[] } | null = null;
  let code: string[] | null = null;

  const flushParagraph = () => {
    if (!paragraph.length) return;
    blocks.push({ kind: "paragraph", text: paragraph.join("\n") });
    paragraph = [];
  };
  const flushList = () => {
    if (!list) return;
    blocks.push({ kind: "list", ordered: list.ordered, items: [...list.items] });
    list = null;
  };
  const flushAll = () => {
    flushParagraph();
    flushList();
  };

  for (const raw of (notes ?? "").split(/\r?\n/)) {
    const line = raw.replace(/\s+$/, "");
    if (code) {
      if (FENCE_PATTERN.test(line)) {
        blocks.push({ kind: "code", text: code.join("\n") });
        code = null;
      } else {
        code.push(raw);
      }
      continue;
    }
    if (FENCE_PATTERN.test(line)) {
      flushAll();
      code = [];
      continue;
    }
    if (!line.trim()) {
      flushAll();
      continue;
    }
    const heading = HEADING_PATTERN.exec(line);
    if (heading) {
      flushAll();
      blocks.push({
        kind: "heading",
        level: heading[1]?.length ?? 1,
        text: heading[2] ?? "",
      });
      continue;
    }
    if (RULE_PATTERN.test(line)) {
      flushAll();
      continue;
    }
    const quote = QUOTE_PATTERN.exec(line);
    if (quote) {
      flushAll();
      paragraph.push(quote[1] ?? "");
      continue;
    }
    const item = LIST_PATTERN.exec(line);
    if (item) {
      flushParagraph();
      if (!list) list = { ordered: /^\s*\d/.test(line), items: [] };
      list.items.push(item[1] ?? "");
      continue;
    }
    flushList();
    paragraph.push(line.trim());
  }

  if (code) blocks.push({ kind: "code", text: code.join("\n") });
  flushAll();
  return blocks;
}

/** 气泡摘要：优先使用变更条目，其次使用正文段落。 */
export function releaseNotesSummary(notes: string | null): string[] {
  const blocks = parseReleaseNotes(notes);
  const bullets = blocks.flatMap((block) => (block.kind === "list" ? block.items : []));
  const paragraphs = blocks.flatMap((block) => (block.kind === "paragraph" ? [block.text] : []));
  const items = bullets.length ? bullets : paragraphs;
  return items
    .map((item) => releaseNotesPlainText(item))
    .filter(Boolean)
    .slice(0, 3);
}
