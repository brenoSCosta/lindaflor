import rehypeShiki from "@shikijs/rehype";
import { Effect } from "effect";
import { useTheme } from "next-themes";
import {
  isValidElement,
  useEffect,
  useId,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { MarkdownHooks, type Components } from "react-markdown";
import rehypeKatex from "rehype-katex";
import remarkGfm from "remark-gfm";
import remarkMath from "remark-math";

import "katex/dist/katex.min.css";

const loadMermaid = () => import("mermaid").then((m) => m.default);

type MermaidDiagramProps = {
  source: string;
};

const MermaidDiagram = ({ source }: MermaidDiagramProps) => {
  const containerRef = useRef<HTMLDivElement>(null);
  const reactId = useId();
  const { resolvedTheme } = useTheme();
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) {
      return undefined;
    }

    let cancelled = false;

    const renderDiagram = async () => {
      const mermaid = await loadMermaid();

      if (cancelled) {
        return;
      }

      const mermaidTheme = resolvedTheme === "dark" ? "dark" : "default";

      mermaid.initialize({
        startOnLoad: false,
        theme: mermaidTheme,
        securityLevel: "loose",
      });

      const renderId = `mermaid-${reactId.replace(/:/g, "")}`;

      container.replaceChildren();

      await Effect.runPromise(
        Effect.tryPromise({
          try: () => mermaid.render(renderId, source),
          catch: (e): Error =>
            e instanceof Error ? e : new Error("Failed to render diagram"),
        }).pipe(
          Effect.tap(({ svg, bindFunctions }) =>
            Effect.sync(() => {
              if (cancelled) {
                return;
              }

              container.innerHTML = svg;
              bindFunctions?.(container);
            }),
          ),
          Effect.catchAll((err) =>
            Effect.sync(() => {
              if (cancelled) {
                return;
              }

              setError(err.message);
            }),
          ),
        ),
      );
    };

    void renderDiagram();

    return () => {
      cancelled = true;
    };
  }, [reactId, resolvedTheme, source]);

  if (error) {
    return (
      <div
        className="not-typeset mermaid-diagram mermaid-diagram-error"
        role="alert"
      >
        {error}
      </div>
    );
  }

  return (
    <div
      ref={containerRef}
      className="not-typeset mermaid-diagram"
      aria-busy="true"
    />
  );
};

function getCodeText(children: ReactNode): string {
  if (typeof children === "string") {
    return children.replace(/\n$/, "");
  }

  return "";
}

const markdownComponents: Components = {
  code({ className, children, ...props }) {
    if (className?.includes("language-mermaid")) {
      const text = getCodeText(children);
      return <MermaidDiagram key={text} source={text} />;
    }

    return (
      <code className={className} {...props}>
        {children}
      </code>
    );
  },
  pre({ children, ...props }) {
    const firstChild = Array.isArray(children) ? children[0] : children;

    if (isValidElement(firstChild) && firstChild.type === MermaidDiagram) {
      return firstChild;
    }

    return <pre {...props}>{children}</pre>;
  },
};

export const Markdown = ({ content }: { content: string }) => {
  return (
    <section className="typeset typeset-docs">
      <MarkdownHooks
        remarkPlugins={[remarkGfm, remarkMath]}
        rehypePlugins={[
          [
            rehypeShiki,
            {
              themes: {
                light: "github-light",
                dark: "github-dark",
              },
              defaultColor: null,
            },
          ],
          rehypeKatex,
        ]}
        components={markdownComponents}
        fallback={null}
      >
        {content}
      </MarkdownHooks>
    </section>
  );
};
