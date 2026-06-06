import Navbar from "@/components/layout/Navbar";
import Footer from "@/components/layout/Footer";

export const metadata = {
  title: "Kaptaind vs semantic-release | Code-Aware Governance",
  description: "Why semantic-release and conventional commits are fragile. Compare Kaptaind's AST-level code analysis to regex-based commit string parsers.",
};

export default function CompareSemanticPage() {
  return (
    <>
      <Navbar />
      <div className="bg-zinc-950 text-zinc-100 min-h-screen py-24 sm:py-32">
        <div className="mx-auto max-w-5xl px-6 lg:px-8">
          <div className="text-center">
            <h1 className="text-4xl font-extrabold tracking-tight sm:text-5xl">
              Kaptaind vs semantic-release
            </h1>
            <p className="mt-4 text-lg text-zinc-400">
              Shift from regex-parsed commit messages to actual code-aware release governance.
            </p>
          </div>

          <div className="mt-16 overflow-x-auto rounded-xl border border-zinc-800 bg-zinc-900/20">
            <table className="min-w-full divide-y divide-zinc-800 text-sm text-left">
              <thead className="bg-zinc-900/60 font-mono text-zinc-400 text-xs">
                <tr>
                  <th className="px-6 py-4">Release Dimension</th>
                  <th className="px-6 py-4">semantic-release</th>
                  <th className="px-6 py-4 text-violet-400">Kaptaind Release Governance</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-zinc-800 text-zinc-300">
                <tr className="hover:bg-zinc-900/10">
                  <td className="px-6 py-4 font-bold">Calculation Base</td>
                  <td className="px-6 py-4">Regex strings matching conventional commits (e.g. feat:, fix:).</td>
                  <td className="px-6 py-4 text-violet-300 font-semibold">AST signature parsing + Structural diff scoring across 12 languages.</td>
                </tr>
                <tr className="hover:bg-zinc-900/10">
                  <td className="px-6 py-4 font-bold">Error Resilience</td>
                  <td className="px-6 py-4">Fragile. A developer typing a wrong prefix will break release versioning.</td>
                  <td className="px-6 py-4 text-violet-300 font-semibold">Absolute. Version calculation looks at the actual API surface change, not commit text.</td>
                </tr>
                <tr className="hover:bg-zinc-900/10">
                  <td className="px-6 py-4 font-bold">Execution Context</td>
                  <td className="px-6 py-4">CI only. Decisions are computed blind in remote jobs without local feedback.</td>
                  <td className="px-6 py-4 text-violet-300 font-semibold">Local-first daemon. Devs receive instant local feedback, sync to portal.</td>
                </tr>
                <tr className="hover:bg-zinc-900/10">
                  <td className="px-6 py-4 font-bold">Policy & Scope Gating</td>
                  <td className="px-6 py-4">None. Relies on simple environment flags and branch checkouts.</td>
                  <td className="px-6 py-4 text-violet-300 font-semibold">Extensive. Enforces branch protections, file patterns, and test coverage hooks.</td>
                </tr>
              </tbody>
            </table>
          </div>

          <div className="mt-16 space-y-6 text-sm text-zinc-400 leading-relaxed max-w-3xl mx-auto">
            <h3 className="text-xl font-bold text-zinc-200">Why Conventional Commits are Not Enough</h3>
            <p>
              Tools like <code>semantic-release</code> solve versioning by forcing developers to adopt rigid commit formats. This creates developer friction and is vulnerable to human error: a single typo in a commit header can trigger a Major version bump, causing breaking downstream pipeline issues.
            </p>
            <p>
              Kaptaind analyzes the code, not the comments. By parsing the abstract syntax tree (AST) of the files changed, Kaptaind knows precisely if a public API method signature was modified, if a dependency lock file was changed, or if a config was edited. It decides semantic bumps with engineering certainty, not regex string matching.
            </p>
          </div>
        </div>
      </div>
      <Footer />
    </>
  );
}
