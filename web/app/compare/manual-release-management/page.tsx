import Navbar from "@/components/layout/Navbar";
import Footer from "@/components/layout/Footer";

export const metadata = {
  title: "Kaptaind vs Manual Release Management | Release Governance",
  description: "Why manual release coordination, spreadsheets, and developer alignment meetings are a tax on delivery. Compare Kaptaind to legacy manual releases.",
};

export default function CompareManualPage() {
  return (
    <>
      <Navbar />
      <div className="bg-zinc-950 text-zinc-100 min-h-screen py-24 sm:py-32">
        <div className="mx-auto max-w-5xl px-6 lg:px-8">
          <div className="text-center">
            <h1 className="text-4xl font-extrabold tracking-tight sm:text-5xl">
              Kaptaind vs Manual Release Management
            </h1>
            <p className="mt-4 text-lg text-zinc-400">
              Eliminate release coordination overhead, meeting taxes, and human version errors.
            </p>
          </div>

          <div className="mt-16 overflow-x-auto rounded-xl border border-zinc-800 bg-zinc-900/20">
            <table className="min-w-full divide-y divide-zinc-800 text-sm text-left">
              <thead className="bg-zinc-900/60 font-mono text-zinc-400 text-xs">
                <tr>
                  <th className="px-6 py-4">Release Dimension</th>
                  <th className="px-6 py-4">Manual Release Management</th>
                  <th className="px-6 py-4 text-violet-400">Kaptaind Release Governance</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-zinc-800 text-zinc-300">
                <tr className="hover:bg-zinc-900/10">
                  <td className="px-6 py-4 font-bold">Coordination Cost</td>
                  <td className="px-6 py-4">High. Requires slack discussions, sync meetings, and checklist confirmations.</td>
                  <td className="px-6 py-4 text-violet-300 font-semibold">Zero. Daemon evaluates repo AST and commits automatically based on local changes.</td>
                </tr>
                <tr className="hover:bg-zinc-900/10">
                  <td className="px-6 py-4 font-bold">Version Calculation</td>
                  <td className="px-6 py-4">Subjective guesses. Developers discuss if a change is major, minor, or patch.</td>
                  <td className="px-6 py-4 text-violet-300 font-semibold">Deterministic. Score engine evaluates AST APIs and structural impact.</td>
                </tr>
                <tr className="hover:bg-zinc-900/10">
                  <td className="px-6 py-4 font-bold">Audit & Traceability</td>
                  <td className="px-6 py-4">Fragmented. Spreadsheets, Jira tickets, and commit logs don&apos;t connect.</td>
                  <td className="px-6 py-4 text-violet-300 font-semibold">Immutable. Local trace JSON files map every change directly to version bumps.</td>
                </tr>
                <tr className="hover:bg-zinc-900/10">
                  <td className="px-6 py-4 font-bold">Policy Gating</td>
                  <td className="px-6 py-4">Manual check. Relies on human memory to confirm tests passed.</td>
                  <td className="px-6 py-4 text-violet-300 font-semibold">Automated. Daemon refuses to commit/push if test checks fail or policies block.</td>
                </tr>
              </tbody>
            </table>
          </div>

          <div className="mt-16 space-y-6 text-sm text-zinc-400 leading-relaxed max-w-3xl mx-auto">
            <h3 className="text-xl font-bold text-zinc-200">Why Manual Coordination is an Anti-Pattern</h3>
            <p>
              Traditional engineering teams spend up to 20% of their time coordinates releases. Meeting updates, branch mergers, version conflicts, and compliance checklists stall release throughput and introduce human risk.
            </p>
            <p>
              Kaptaind acts as an autonomous policy gate. The system continually parses your workspace, measures the semantic impact of changes, validates compliance policies (like tests and file blocks), writes version manifests, and pushes signed commits, eliminating the release coordinator role entirely.
            </p>
          </div>
        </div>
      </div>
      <Footer />
    </>
  );
}
