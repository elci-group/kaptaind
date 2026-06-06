import Navbar from "@/components/layout/Navbar";
import Footer from "@/components/layout/Footer";

export const metadata = {
  title: "Kaptaind vs GitHub Actions Only | Pipeline Intelligence",
  description: "Compare running bare GitHub Actions workflows to Kaptaind's local-first release governance model. Why CI pipelines need code-aware decision gates.",
};

export default function CompareActionsPage() {
  return (
    <>
      <Navbar />
      <div className="bg-zinc-950 text-zinc-100 min-h-screen py-24 sm:py-32">
        <div className="mx-auto max-w-5xl px-6 lg:px-8">
          <div className="text-center">
            <h1 className="text-4xl font-extrabold tracking-tight sm:text-5xl">
              Kaptaind vs GitHub Actions (Only)
            </h1>
            <p className="mt-4 text-lg text-zinc-400">
              Add release intelligence and local-first governance to your CI/CD pipelines.
            </p>
          </div>

          <div className="mt-16 overflow-x-auto rounded-xl border border-zinc-800 bg-zinc-900/20">
            <table className="min-w-full divide-y divide-zinc-800 text-sm text-left">
              <thead className="bg-zinc-900/60 font-mono text-zinc-400 text-xs">
                <tr>
                  <th className="px-6 py-4">Release Dimension</th>
                  <th className="px-6 py-4">GitHub Actions Workflows Only</th>
                  <th className="px-6 py-4 text-violet-400">Kaptaind Release Governance</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-zinc-800 text-zinc-300">
                <tr className="hover:bg-zinc-900/10">
                  <td className="px-6 py-4 font-bold">Feedback Loop</td>
                  <td className="px-6 py-4">Delayed. Developers must push code, wait for a VM to boot, run, and fail.</td>
                  <td className="px-6 py-4 text-violet-300 font-semibold">Instant. Local daemon analyzes files and evaluates policies directly on the developer&apos;s laptop.</td>
                </tr>
                <tr className="hover:bg-zinc-900/10">
                  <td className="px-6 py-4 font-bold">Release Audit Traces</td>
                  <td className="px-6 py-4">Scattered. Found in build logs, runner transcripts, and git commit objects.</td>
                  <td className="px-6 py-4 text-violet-300 font-semibold">Structured. JSON traces map change metrics, AST analysis, and policy approvals in one manifest.</td>
                </tr>
                <tr className="hover:bg-zinc-900/10">
                  <td className="px-6 py-4 font-bold">Policy Customization</td>
                  <td className="px-6 py-4">Hard. Requires writing custom shell commands or composite actions in YAML.</td>
                  <td className="px-6 py-4 text-violet-300 font-semibold">Built-in. Enforces branch protection, minimum tests, banned files via typed configs.</td>
                </tr>
                <tr className="hover:bg-zinc-900/10">
                  <td className="px-6 py-4 font-bold">Inference Cost Control</td>
                  <td className="px-6 py-4">None. Runs naively on each commit, leading to high API token costs if using LLMs.</td>
                  <td className="px-6 py-4 text-violet-300 font-semibold">Excellent. Enterprise tier batches, caches, and selects models to reduce token use by 60%.</td>
                </tr>
              </tbody>
            </table>
          </div>

          <div className="mt-16 space-y-6 text-sm text-zinc-400 leading-relaxed max-w-3xl mx-auto">
            <h3 className="text-xl font-bold text-zinc-200">The Limits of CI-Only Pipelines</h3>
            <p>
              GitHub Actions is a powerful execution engine but it lacks semantic awareness. It runs shell tasks inside container VMs but has no idea if the code being committed complies with release version boundaries or secure development policies.
            </p>
            <p>
              Kaptaind complements GitHub Actions. By running a local-first daemon or as an Action gate, Kaptaind verifies version decisions, checks policy requirements, and writes traces before pushing code to remote branches, saving developers minutes of waiting time and protecting main branches from invalid release signals.
            </p>
          </div>
        </div>
      </div>
      <Footer />
    </>
  );
}
