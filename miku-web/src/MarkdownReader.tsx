import { useEffect, useId, useState, type ReactNode } from "react";
import { useNavigate } from "react-router-dom";
import ReactMarkdown from "react-markdown";
import rehypeHighlight from "rehype-highlight";
import rehypeRaw from "rehype-raw";
import rehypeSanitize from "rehype-sanitize";
import remarkGfm from "remark-gfm";
import mermaid from "mermaid";
import "highlight.js/styles/github-dark.css";

function noteHref(target: string): string {
  const trimmed = target.trim();
  const path = trimmed.endsWith(".md") ? trimmed : trimmed + ".md";
  return "/p/" + encodeURIComponent(path);
}

function expandWikiLinks(markdown: string): string {
  return markdown.replace(/(?<!!)\[\[([^\]|]+)(?:\|([^\]]+))?\]\]/g, (_match, target: string, label?: string) => "[" + (label?.trim() || target.trim()) + "](" + noteHref(target) + ")");
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
  return <div className="mermaid-diagram" dangerouslySetInnerHTML={{ __html: svg }} />;
}

function textContent(children: ReactNode): string {
  if (typeof children === "string") return children;
  if (Array.isArray(children)) return children.map(textContent).join("");
  if (children && typeof children === "object" && "props" in children) {
    return textContent((children as { props?: { children?: ReactNode } }).props?.children);
  }
  return "";
}

export function MarkdownReader({ value }: { value: string }) {
  const navigate = useNavigate();
  return (
    <article className="markdown-reader">
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        rehypePlugins={[rehypeRaw, rehypeSanitize, rehypeHighlight]}
        components={{
          a: ({ href, children, ...props }) => {
            const internal = href?.startsWith("/p/");
            return (
              <a
                {...props}
                href={href}
                onClick={(event) => {
                  if (!internal || !href) return;
                  event.preventDefault();
                  navigate(href);
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
                {children}
              </blockquote>
            );
          },
          code: ({ className, children, ...props }) => {
            const source = String(children).replace(/\n$/, "");
            if (className === "language-mermaid") return <MermaidChart source={source} />;
            return (
              <code className={className} {...props}>
                {children}
              </code>
            );
          }
        }}
      >
        {expandWikiLinks(value)}
      </ReactMarkdown>
    </article>
  );
}
