import { Fragment, useMemo } from "react";
import { parseReleaseInline, parseReleaseNotes } from "@/lib/release-notes";
import type { ReleaseInline } from "@/lib/release-notes";
import { cn } from "@/lib/utils";

function InlineText({ parts }: { parts: ReleaseInline[] }) {
  return (
    <>
      {parts.map((part, index) => {
        if (part.kind === "code")
          return (
            <code
              key={index}
              className="rounded-sm bg-muted px-1 py-0.5 font-mono text-xs break-words text-foreground"
            >
              {part.text}
            </code>
          );
        if (part.kind === "strong")
          return (
            <strong key={index} className="font-semibold">
              {part.text}
            </strong>
          );
        if (part.kind === "emphasis")
          return (
            <em key={index} className="italic">
              {part.text}
            </em>
          );
        return <Fragment key={index}>{part.text}</Fragment>;
      })}
    </>
  );
}

/**
 * 发布说明渲染：保留标题层级、列表与行内代码，不加载远程图片与 HTML。
 * `omitVersion` 用于隐藏与顶部版本号重复的发布标题，保证版本号只出现一次。
 */
export function ReleaseNotes({
  notes,
  className,
  omitVersion,
}: {
  notes: string | null;
  className?: string;
  omitVersion?: string | null;
}) {
  const blocks = useMemo(() => {
    const parsed = parseReleaseNotes(notes);
    if (!omitVersion) return parsed;
    const [first] = parsed;
    if (first?.kind === "heading" && first.text.includes(omitVersion)) return parsed.slice(1);
    return parsed;
  }, [notes, omitVersion]);
  if (!blocks.length) return null;
  return (
    <div className={cn("space-y-3 text-sm leading-relaxed", className)}>
      {blocks.map((block, index) => {
        if (block.kind === "heading") {
          if (block.level <= 1)
            return (
              <h3 key={index} className="text-lg font-semibold text-foreground">
                {block.text}
              </h3>
            );
          if (block.level === 2)
            return (
              <h4
                key={index}
                className="border-b border-border pb-1.5 text-base font-semibold text-foreground"
              >
                {block.text}
              </h4>
            );
          return (
            <h5 key={index} className="text-sm font-semibold text-foreground">
              {block.text}
            </h5>
          );
        }
        if (block.kind === "list") {
          const List = block.ordered ? "ol" : "ul";
          return (
            <List
              key={index}
              className={
                block.ordered
                  ? "list-decimal space-y-1.5 pl-5 marker:text-muted-foreground"
                  : "list-disc space-y-1.5 pl-5 marker:text-muted-foreground"
              }
            >
              {block.items.map((item, itemIndex) => (
                <li key={itemIndex} className="break-words text-foreground">
                  <InlineText parts={parseReleaseInline(item)} />
                </li>
              ))}
            </List>
          );
        }
        if (block.kind === "code")
          return (
            <pre
              key={index}
              className="overflow-x-auto rounded-md bg-muted p-3 font-mono text-xs whitespace-pre-wrap text-foreground"
            >
              <code>{block.text}</code>
            </pre>
          );
        return (
          <p key={index} className="break-words whitespace-pre-line text-foreground">
            <InlineText parts={parseReleaseInline(block.text)} />
          </p>
        );
      })}
    </div>
  );
}
