import Link from "next/link";
import Navbar from "@/components/layout/Navbar";
import Footer from "@/components/layout/Footer";

export const metadata = {
  title: "Kaptaind | Case Study — Rust Ecosystem",
  description:
    "How Kaptaind automates semantic versioning for Rust projects with Cargo.toml awareness, crate boundary detection, and crates.io-compatible semver.",
};

export default function RustEcosystemCaseStudy() {
  return (
    <>
      <Navbar />
      <div className="bg-zinc-950 text-zinc-100 min-h-screen py-24 sm:py-32">
        <div className="mx-auto max-w-4xl px-6 lg:px-8">
          <div className="text-center">
            <span className="inline-flex items-center rounded-full bg-orange-500/10 px-3 py-1 text-xs font-medium text-orange-400 ring-1 ring-inset ring-orange-500/20 mb-4">
              Case Study
            </span>
            <h1 className="text-4xl font-extrabold tracking-tight sm:text-5xl">
              Rust Ecosystem
            </h1>
            <p className="mt-4 text-lg text-zinc-400">
              Semantic versioning that understands Cargo, crates, and public API boundaries.
            </p>
          </div>

          <div className="mt-16 space-y-16">
            {/* Section 1 */}
            <section>
              <h2 className="text-2xl font-bold text-zinc-100 mb-4">The Challenge</h2>
              <p className="text-zinc-400 leading-relaxed">
                Rust projects rely heavily on semantic versioning for dependency resolution via Cargo. A missed major bump in a public crate can break downstream consumers. Yet most teams still version manually — relying on developers to remember whether a change to <code className="bg-zinc-900 px-1 py-0.5 rounded text-yellow-400 font-mono">pub fn</code> signatures is breaking.
              </p>
              <p className="mt-4 text-zinc-400 leading-relaxed">
                In a typical workspace with multiple crates, the risk compounds. A refactor in a core utility crate can silently alter public types, and without AST-aware detection, the version bump is often a guess.
              </p>
            </section>

            {/* Section 2 */}
            <section className="border border-zinc-800 rounded-2xl p-8 bg-zinc-900/40">
              <h2 className="text-2xl font-bold text-zinc-100 mb-4">How Kaptaind Helps</h2>
              <div className="space-y-6">
                <div className="flex items-start gap-4">
                  <div className="flex h-8 w-8 items-center justify-center rounded-lg bg-violet-500/10 text-violet-400 font-bold text-sm shrink-0">1</div>
                  <div>
                    <h3 className="text-lg font-semibold text-zinc-200">Cargo.toml Integration</h3>
                    <p className="mt-1 text-sm text-zinc-400 leading-relaxed">
                      Kaptaind detects <code className="bg-zinc-900 px-1 py-0.5 rounded text-yellow-400 font-mono">Cargo.toml</code> and <code className="bg-zinc-900 px-1 py-0.5 rounded text-yellow-400 font-mono">Cargo.lock</code> changes as dependency dimension events. It parses runtime configs like <code className="bg-zinc-900 px-1 py-0.5 rounded text-yellow-400 font-mono">.service</code> files and Docker deployments for the runtime score.
                    </p>
                  </div>
                </div>
                <div className="flex items-start gap-4">
                  <div className="flex h-8 w-8 items-center justify-center rounded-lg bg-violet-500/10 text-violet-400 font-bold text-sm shrink-0">2</div>
                  <div>
                    <h3 className="text-lg font-semibold text-zinc-200">AST-Aware API Detection</h3>
                    <p className="mt-1 text-sm text-zinc-400 leading-relaxed">
                      The Rust language adapter (confidence 1.0) scans for <code className="bg-zinc-900 px-1 py-0.5 rounded text-yellow-400 font-mono">pub fn</code>, <code className="bg-zinc-900 px-1 py-0.5 rounded text-yellow-400 font-mono">pub struct</code>, <code className="bg-zinc-900 px-1 py-0.5 rounded text-yellow-400 font-mono">pub trait</code>, and <code className="bg-zinc-900 px-1 py-0.5 rounded text-yellow-400 font-mono">pub enum</code> additions or modifications. Breaking changes — such as removing a public symbol — trigger an automatic major bump.
                    </p>
                  </div>
                </div>
                <div className="flex items-start gap-4">
                  <div className="flex h-8 w-8 items-center justify-center rounded-lg bg-violet-500/10 text-violet-400 font-bold text-sm shrink-0">3</div>
                  <div>
                    <h3 className="text-lg font-semibold text-zinc-200">Workspace-Aware Staging</h3>
                    <p className="mt-1 text-sm text-zinc-400 leading-relaxed">
                      Using pattern-based staging, Kaptaind can scope commits to individual crates within a workspace. Exclude globs prevent lockfile noise from leaking into unrelated crate commits.
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
                  <div className="text-3xl font-extrabold text-emerald-400">0</div>
                  <div className="text-xs text-zinc-500 mt-1 uppercase tracking-wider">Missed Major Bumps</div>
                </div>
                <div className="border border-zinc-800 rounded-xl p-6 bg-zinc-900/60 text-center">
                  <div className="text-3xl font-extrabold text-violet-400">1.0</div>
                  <div className="text-xs text-zinc-500 mt-1 uppercase tracking-wider">Rust Adapter Confidence</div>
                </div>
                <div className="border border-zinc-800 rounded-xl p-6 bg-zinc-900/60 text-center">
                  <div className="text-3xl font-extrabold text-zinc-100">&lt;2s</div>
                  <div className="text-xs text-zinc-500 mt-1 uppercase tracking-wider">Analysis Latency</div>
                </div>
              </div>
              <p className="mt-6 text-zinc-400 leading-relaxed">
                Teams running Kaptaind on Rust codebases report faster release cycles and fewer downstream breakages. The deterministic semver rules mean every crate release is explainable — with an immutable trace stored under <code className="bg-zinc-900 px-1 py-0.5 rounded text-yellow-400 font-mono">.kaptaind/traces/</code>.
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
