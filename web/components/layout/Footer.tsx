export default function Footer() {
  return (
    <footer className="border-t border-zinc-800 bg-zinc-950">
      <div className="mx-auto max-w-7xl px-4 py-12 sm:px-6 lg:px-8">
        <div className="flex flex-col items-center justify-between gap-4 sm:flex-row">
          <p className="text-sm text-zinc-400">
            Kaptaind Pro — Automated semantic versioning, enhanced by AI.
          </p>
          <p className="text-sm text-zinc-500">
            Built with Next.js and Claude
          </p>
        </div>
      </div>
    </footer>
  );
}
