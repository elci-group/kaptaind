"use client";

import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";

export default function MarkdownRenderer({ content }: { content: string }) {
  return (
    <ReactMarkdown
      remarkPlugins={[remarkGfm]}
      components={{
        h1: ({ children }) => (
          <h1 className="text-3xl font-extrabold text-zinc-100 mt-12 mb-6 border-b border-zinc-800 pb-4">
            {children}
          </h1>
        ),
        h2: ({ children }) => (
          <h2 className="text-xl font-bold text-zinc-100 mt-10 mb-4">
            {children}
          </h2>
        ),
        h3: ({ children }) => (
          <h3 className="text-lg font-semibold text-zinc-200 mt-8 mb-3">
            {children}
          </h3>
        ),
        p: ({ children }) => (
          <p className="text-sm text-zinc-400 leading-relaxed mb-4">
            {children}
          </p>
        ),
        ul: ({ children }) => (
          <ul className="list-disc list-inside text-sm text-zinc-400 mb-4 space-y-1">
            {children}
          </ul>
        ),
        ol: ({ children }) => (
          <ol className="list-decimal list-inside text-sm text-zinc-400 mb-4 space-y-1">
            {children}
          </ol>
        ),
        li: ({ children }) => (
          <li className="text-sm text-zinc-400 leading-relaxed">
            {children}
          </li>
        ),
        code: ({ children, className }) => {
          const isInline = !className;
          return isInline ? (
            <code className="rounded bg-zinc-800 px-1.5 py-0.5 text-xs text-violet-300 font-mono">
              {children}
            </code>
          ) : (
            <pre className="rounded-lg bg-zinc-900 border border-zinc-800 p-4 overflow-x-auto mb-6">
              <code className="text-xs text-zinc-300 font-mono leading-relaxed block">
                {children}
              </code>
            </pre>
          );
        },
        table: ({ children }) => (
          <div className="overflow-x-auto mb-6">
            <table className="min-w-full divide-y divide-zinc-800 text-sm">
              {children}
            </table>
          </div>
        ),
        thead: ({ children }) => (
          <thead className="bg-zinc-900/60 font-mono text-zinc-400 text-xs">
            {children}
          </thead>
        ),
        tbody: ({ children }) => (
          <tbody className="divide-y divide-zinc-800 text-zinc-300">
            {children}
          </tbody>
        ),
        th: ({ children }) => (
          <th className="px-4 py-3 text-left font-semibold">{children}</th>
        ),
        td: ({ children }) => (
          <td className="px-4 py-3 text-zinc-400">{children}</td>
        ),
        blockquote: ({ children }) => (
          <blockquote className="border-l-4 border-violet-500/30 pl-4 italic text-zinc-400 mb-4">
            {children}
          </blockquote>
        ),
        a: ({ children, href }) => (
          <a href={href} className="text-violet-400 hover:text-violet-300 underline">
            {children}
          </a>
        ),
        strong: ({ children }) => (
          <strong className="text-zinc-200 font-semibold">{children}</strong>
        ),
      }}
    >
      {content}
    </ReactMarkdown>
  );
}
