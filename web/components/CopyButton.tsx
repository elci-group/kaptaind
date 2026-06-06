"use client";

export default function CopyButton({ text }: { text: string }) {
  return (
    <button
      className="ml-4 bg-zinc-800 hover:bg-zinc-700 text-zinc-200 font-sans text-xs px-3 py-1.5 rounded transition-all active:scale-95"
      onClick={() => {
        navigator.clipboard.writeText(text);
      }}
    >
      Copy
    </button>
  );
}
