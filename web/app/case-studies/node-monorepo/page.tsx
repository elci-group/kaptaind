import Link from "next/link";
import Navbar from "@/components/layout/Navbar";
import Footer from "@/components/layout/Footer";

export const metadata = {
  title: "Kaptaind | Case Study — Node.js Monorepo",
  description:
    "How Kaptaind handles Node.js monorepos with package.json awareness, workspace boundary detection, and per-package semantic versioning.",
};

export default function NodeMonorepoCaseStudy() {
  return (
    <>
      <Navbar />
      <div className="bg-zinc-950 text-zinc-100 min-h-screen py-24 sm:py-32">
        <div className="mx-auto max-w-4xl px-6 lg:px-8">
          <div className="text-center">
            <span className="inline-flex items-center rounded-full bg-emerald-500/10 px-3 py-1 text-xs font-medium text-emerald-400 ring-1 ring-inset ring-emerald-500/20 mb-4">
              Case Study
            </span>
            <h1 className="text-4xl font-extrabold tracking-tight sm:text-5xl">
              Node.js Monorepo
            </h1>
            <p className="mt-4 text-lg text-zinc-400">
              Per-package versioning for Lerna, Nx, and pnpm workspaces without the coordination overhead.
            </p>
          </div>

          <div className="mt-16 space-y-16">
            {/* Section 1 */}
            <section>
              <h2 className="text-2xl font-bold text-zinc-100 mb-4">The Challenge</h2>
              <p className="text-zinc-400 leading-relaxed">
                Node.js monorepos are powerful but versioning them is painful. A single PR can touch five packages, yet each needs its own semver bump. Tools like Lerna and Changesets require manual change-file authoring, and it is easy to forget a package or misjudge whether a type signature change is breaking.
              </p>
              <p className="mt-4 text-zinc-400 leading-relaxed">
                In large Nx or pnpm workspaces, the problem scales further. Lockfile changes (<code className="bg-zinc-900 px-1 py-0.5 rounded text-yellow-400 font-mono">package-lock.json</code>, <code className="bg-zinc-900 px-1 py-0.5 rounded text-yellow-400 font-mono">pnpm-lock.yaml</code>) affect every package, but you do not want to version-bump the entire repo for a dev-dependency update.
              </p>
            </section>

            {/* Section 2 */}
            <section className="border border-zinc-800 rounded-2xl p-8 bg-zinc-900/40">
              <h2 className="text-2xl font-bold text-zinc-100 mb-4">How Kaptaind Helps</h2>
              <div className="space-y-6">
                <div className="flex items-start gap-4">
                  <div className="flex h-8 w-8 items-center justify-center rounded-lg bg-violet-500/10 text-violet-400 font-bold text-sm shrink-0">1</div>
                  <div>
                    <h3 className="text-lg font-semibold text-zinc-200">package.json & Lockfile Parsing</h3>
                    <p className="mt-1 text-sm text-zinc-400 leading-relaxed">
                      Kaptaind recognizes <code className="bg-zinc-900 px-1 py-0.5 rounded text-yellow-400 font-mono">package.json</code>, <code className="bg-zinc-900 px-1 py-0.5 rounded text-yellow-400 font-mono">package-lock.json</code>, <code className="bg-zinc-900 px-1 py-0.5 rounded text-yellow-400 font-mono">pnpm-lock.yaml</code>, <code className="bg-zinc-900 px-1 py-0.5 rounded text-yellow-400 font-mono">yarn.lock</code>, and <code className="bg-zinc-900 px-1 py-0.5 rounded text-yellow-400 font-mono">bun.lockb</code>. It separates production dependency shifts (higher impact) from dev-dependency churn (lower impact) when computing the dependency score.
                    </p>
                  </div>
                </div>
                <div className="flex items-start gap-4">
                  <div className="flex h-8 w-8 items-center justify-center rounded-lg bg-violet-500/10 text-violet-400 font-bold text-sm shrink-0">2</div>
                  <div>
                    <h3 className="text-lg font-semibold text-zinc-200">TypeScript & JavaScript Adapters</h3>
                    <p className="mt-1 text-sm text-zinc-400 leading-relaxed">
                      The TypeScript adapter (confidence 0.9) detects <code className="bg-zinc-900 px-1 py-0.5 rounded text-yellow-400 font-mono">export</code> additions, interface changes, and React component prop modifications. The JavaScript adapter (confidence 0.7) provides fallback line-based signature scanning for untyped code.
                    </p>
                  </div>
                </div>
                <div className="flex items-start gap-4">
                  <div className="flex h-8 w-8 items-center justify-center rounded-lg bg-violet-500/10 text-violet-400 font-bold text-sm shrink-0">3</div>
                  <div>
                    <h3 className="text-lg font-semibold text-zinc-200">Selective Staging per Workspace</h3>
                    <p className="mt-1 text-sm text-zinc-400 leading-relaxed">
                      Using pattern-based staging, Kaptaind scopes commits to individual workspace packages. Change <code className="bg-zinc-900 px-1 py-0.5 rounded text-yellow-400 font-mono">packages/auth</code>? Only auth and the root lockfile are staged. The rest of the monorepo stays untouched.
                    </p>
                  </div>
                </div>
              </div>
            </section>

            {/* Section 3 */}
            <section>
              <h2 className="text-2xl font-bold text-zinc-100 mb-4">Results</h2>
              <div className="grid gap-4 sm:grid-cols-3">
                <div className="border border-zinc-800 rounded-xl p-6 bg-zinc-900/60 text-center">
                  <div className="text-3xl font-extrabold text-emerald-400">~80%</div>
                  <div className="text-xs text-zinc-500 mt-1 uppercase tracking-wider">Less Versioning Overhead</div>
                </div>
                <div className="border border-zinc-800 rounded-xl p-6 bg-zinc-900/60 text-center">
                  <div className="text-3xl font-extrabold text-violet-400">0.9</div>
                  <div className="text-xs text-zinc-500 mt-1 uppercase tracking-wider">TS Adapter Confidence</div>
                </div>
                <div className="border border-zinc-800 rounded-xl p-6 bg-zinc-900/60 text-center">
                  <div className="text-3xl font-extrabold text-zinc-100">Per-Package</div>
                  <div className="text-xs text-zinc-500 mt-1 uppercase tracking-wider">Scoped Commits</div>
                </div>
              </div>
              <p className="mt-6 text-zinc-400 leading-relaxed">
                Teams running Kaptaind in pnpm and Nx workspaces no longer manually curate Changeset files. The daemon watches every package directory, clusters changes by proximity, and versions each package independently based on its actual API and dependency impact.
              </p>
            </section>
          </div>

          <div className="mt-16 flex flex-col sm:flex-row items-center justify-center gap-4">
            <Link
              href="/case-studies"
              className="text-sm font-semibold text-zinc-400 hover:text-zinc-200 transition-colors"
            >
              ← All case studies
            </Link>
            <Link
              href="/download"
              className="rounded-lg bg-violet-600 px-6 py-3 text-sm font-semibold text-white shadow hover:bg-violet-500 transition-colors"
            >
              Install Kaptaind
            </Link>
          </div>
        </div>
      </div>
      <Footer />
    </>
  );
}
