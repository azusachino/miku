import { Children, cloneElement, isValidElement, useEffect, useId, useState, type ReactNode } from "react";
import { useNavigate } from "react-router-dom";
import ReactMarkdown from "react-markdown";
import rehypeHighlight from "rehype-highlight";
import rehypeRaw from "rehype-raw";
import rehypeSanitize from "rehype-sanitize";
import remarkGfm from "remark-gfm";
import rehypeKatex from "rehype-katex";
import remarkMath from "remark-math";
import mermaid from "mermaid";
import { headingSlug } from "./ui";
import "highlight.js/styles/github-dark.css";
import "katex/dist/katex.min.css";

export function noteHref(target: string): string {
  const trimmed = target.trim();
  const path = trimmed.endsWith(".md") ? trimmed : trimmed + ".md";
  return "/p/" + path.split("/").map(encodeURIComponent).join("/");
}

export function resolveMarkdownHref(href: string, currentPath: string): string | null {
  const trimmed = href.trim();
  if (!trimmed || trimmed.startsWith("#") || /^[a-z][a-z\d+.-]*:/i.test(trimmed)) return null;
  if (trimmed.startsWith("/p/") || trimmed.startsWith("/tags/") || trimmed.startsWith("/assets/")) return trimmed;
  const [target, hash] = trimmed.split("#", 2);
  if (!target || (/\.[a-z\d]+$/i.test(target) && !target.endsWith(".md"))) return null;
  const base = currentPath.split("/").slice(0, -1);
  const normalized: string[] = [];
  for (const segment of [...base, ...target.split("/")]) {
    if (!segment || segment === ".") continue;
    if (segment === "..") normalized.pop();
    else normalized.push(segment);
  }
  return noteHref(normalized.join("/")) + (hash ? `#${hash}` : "");
}

export function expandWikiLinks(markdown: string): string {
  const withEmbeds = markdown.replace(/!\[\[([^\]|]+)(?:\|([^\]]+))?\]\]/g, (_match, target: string, label?: string) => `> Embedded note: [${label?.trim() || target.trim()}](${noteHref(target)})`);
  return withEmbeds.replace(/(?<!!)\[\[([^\]|]+)(?:\|([^\]]+))?\]\]/g, (_match, target: string, label?: string) => "[" + (label?.trim() || target.trim()) + "](" + noteHref(target) + ")");
}

export function expandInlineTags(markdown: string): string {
  const segments = markdown.split(/(```[\s\S]*?```|`[^`]*`)/g);
  return segments
    .map((segment, index) => {
      if (index % 2 === 1) return segment;
      return segment.replace(/(^|[\s(])#([A-Za-z][\w/-]*)/g, (_match, prefix: string, tag: string) => `${prefix}[#${tag}](/tags/${encodeURIComponent(tag)})`);
    })
    .join("");
}

function MermaidChart({ source }: { source: string }) {
  const rawId = useId();
  const id = "miku-mermaid-" + rawId.replace(/[^a-zA-Z0-9_-]/g, "");
  const [svg, setSvg] = useState("");
  const [error, setError] = useState("");

  useEffect(() => {
    let active = true;
    mermaid.initialize({ startOnLoad: false, securityLevel: "strict", theme: "neutral" });
    mermaid
      .render(id, source)
      .then(({ svg: rendered }) => {
        if (active) setSvg(rendered);
      })
      .catch((renderError: unknown) => {
        if (active) setError(renderError instanceof Error ? renderError.message : "Mermaid diagram failed");
      });
    return () => {
      active = false;
    };
  }, [id, source]);

  if (error) return <pre className="mermaid-error">{error}</pre>;
  return svg ? <div className="mermaid-diagram" dangerouslySetInnerHTML={{ __html: svg }} /> : <div className="mermaid-diagram mermaid-loading">Rendering diagram…</div>;
}

function textContent(children: ReactNode): string {
  if (typeof children === "string") return children;
  if (Array.isArray(children)) return children.map(textContent).join("");
  if (children && typeof children === "object" && "props" in children) {
    return textContent((children as { props?: { children?: ReactNode } }).props?.children);
  }
  return "";
}

function stripAdmonitionMarker(children: ReactNode): ReactNode {
  let removed = false;
  const strip = (value: ReactNode): ReactNode => {
    if (typeof value === "string") {
      if (removed) return value;
      removed = true;
      return value.replace(/^\[!(NOTE|TIP|IMPORTANT|WARNING|CAUTION)\]\s*/i, "");
    }
    if (Array.isArray(value)) return value.map(strip);
    if (isValidElement(value)) {
      const props = value.props as { children?: ReactNode };
      if (props.children !== undefined) return cloneElement(value, {}, strip(props.children));
    }
    return value;
  };
  return Children.map(children, strip);
}

export function MarkdownReader({ value, path = "" }: { value: string; path?: string }) {
  const navigate = useNavigate();
  return (
    <article className="markdown-reader">
      <ReactMarkdown
        remarkPlugins={[remarkGfm, remarkMath]}
        rehypePlugins={[rehypeRaw, rehypeSanitize, rehypeHighlight, rehypeKatex]}
        components={{
          a: ({ href, children, ...props }) => {
            const resolvedHref = href && path ? resolveMarkdownHref(href, path) ?? href : href;
            const internal = resolvedHref?.startsWith("/p/") || resolvedHref?.startsWith("/tags/");
            return (
              <a
                {...props}
                href={resolvedHref}
                onClick={(event) => {
                  if (!internal || !resolvedHref) return;
                  event.preventDefault();
                  navigate(resolvedHref);
                }}
              >
                {children}
              </a>
            );
          },
          blockquote: ({ children, ...props }) => {
              const content = textContent(children);
              const match = content.match(/^\[!(NOTE|TIP|IMPORTANT|WARNING|CAUTION)\]/i);
              return (
                <blockquote {...props} className={match ? "admonition admonition-" + match[1].toLowerCase() : undefined}>
                {match ? stripAdmonitionMarker(children) : children}
                </blockquote>
              );
            },
          p: ({ children, ...props }) => {
            const match = textContent(children).match(/^\[!(NOTE|TIP|IMPORTANT|WARNING|CAUTION)\]\s*/i);
            return <p {...props} className={match ? "admonition admonition-" + match[1].toLowerCase() : undefined}>{match ? stripAdmonitionMarker(children) : children}</p>;
          },
          h2: ({ children, ...props }) => <h2 {...props} id={headingSlug(textContent(children))}>{children}</h2>,
          h3: ({ children, ...props }) => <h3 {...props} id={headingSlug(textContent(children))}>{children}</h3>,
          h4: ({ children, ...props }) => <h4 {...props} id={headingSlug(textContent(children))}>{children}</h4>,
          code: ({ className, children, ...props }) => {
            const source = String(children).replace(/\n$/, "");
            if (/\blanguage-mermaid\b/.test(className ?? "")) return <MermaidChart source={source} />;
            return (
              <code className={className} {...props}>
                {children}
              </code>
            );
          }
        }}
      >
        {expandInlineTags(expandWikiLinks(value))}
      </ReactMarkdown>
    </article>
  );
}
