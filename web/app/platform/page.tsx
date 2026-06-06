import Link from "next/link";
import Navbar from "@/components/layout/Navbar";
import Footer from "@/components/layout/Footer";

export const metadata = {
  title: "Kaptaind | Platform Architecture",
  description:
    "Deep dive into Kaptaind's architecture: local daemon, cluster engine, five-dimensional diff analysis, semantic versioning, commit orchestration, and web dashboard.",
};

const dimensions = [
  {
    name: "Structural",
    weight: "0.5×",
    desc: "Event density, path spread, and file churn. Measures how broadly a change touches the codebase.",
  },
  {
    name: "API Surface",
    weight: "0.2×",
    desc: "AST-aware detection of public symbol additions, modifications, and removals across 12 language adapters.",
  },
  {
    name: "Dependencies",
    weight: "0.15×",
    desc: "Lockfile and manifest parsing for Cargo, npm, pnpm, yarn, Poetry, Gradle, and Swift Package Manager.",
  },
  {
    name: "Runtime",
    weight: "0.1×",
    desc: "Detection of Docker, K8s, Helm, Vercel, Netlify, and mobile config changes that affect deployment.",
  },
  {
    name: "Bundle Size",
    weight: "0.05×",
    desc: "Optional opt-in scoring that measures build output deltas and clamps impact to [0, 1].",
  },
];

export default function PlatformPage() {
  return (
    <>
      <Navbar />
      <div className="bg-zinc-950 text-zinc-100 min-h-screen py-24 sm:py-32">
        <div className="mx-auto max-w-7xl px-6 lg:px-8">
          <div className="mx-auto max-w-4xl text-center">
            <h1 className="text-4xl font-extrabold tracking-tight sm:text-5xl">
              Platform Architecture
            </h1>
            <p className="mt-4 text-lg text-zinc-400">
              Kaptaind is built as a local-first, Rust-native release governance engine with six core subsystems.
            </p>
          </div>

          {/* ASCII / CSS Architecture Diagram */}
          <div className="mt-16 border border-zinc-800 rounded-2xl p-8 bg-zinc-900/40 max-w-5xl mx-auto backdrop-blur-sm">
            <h3 className="text-lg font-semibold text-zinc-200 mb-8 text-center">
              System Architecture
            </h3>
            <div className="flex flex-col gap-4 text-xs font-mono">
              {/* Layer 1: Watcher */}
              <div className="flex items-center gap-4">
                <div className="flex-1 border border-violet-500/30 rounded-lg p-4 bg-violet-500/5 text-center">
                  <span className="text-violet-400 font-bold">Daemon (Local Watcher)</span>
                  <p className="text-zinc-500 mt-1">inotify / FSEvents → FsEvent → MPSC channel</p>
                </div>
              </div>
              <div className="text-center text-zinc-600">↓</div>

              {/* Layer 2: Cluster Engine */}
              <div className="flex items-center gap-4">
                <div className="flex-1 border border-violet-500/30 rounded-lg p-4 bg-violet-500/5 text-center">
                  <span className="text-violet-400 font-bold">Cluster Engine</span>
                  <p className="text-zinc-500 mt-1">Temporal window (default 5s) + path grouping</p>
                </div>
              </div>
              <div className="text-center text-zinc-600">↓</div>

              {/* Layer 3: Diff Analysis */}
              <div className="grid grid-cols-1 sm:grid-cols-5 gap-3">
                {dimensions.map((d) => (
                  <div
                    key={d.name}
                    className="border border-zinc-700 rounded-lg p-3 bg-zinc-950 text-center"
                  >
                    <span className="text-zinc-300 font-bold block">{d.name}</span>
                    <span className="text-violet-400">{d.weight}</span>
                  </div>
                ))}
              </div>
              <div className="text-center text-zinc-600">↓</div>

              {/* Layer 4: Version + Commit */}
              <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
                <div className="border border-emerald-500/20 rounded-lg p-4 bg-emerald-500/5 text-center">
                  <span className="text-emerald-400 font-bold">Version Semver</span>
                  <p className="text-zinc-500 mt-1">Breaking → Major | Added API / score &gt;0.6 → Minor | score &gt;0.1 → Patch</p>
                </div>
                <div className="border border-emerald-500/20 rounded-lg p-4 bg-emerald-500/5 text-center">
                  <span className="text-emerald-400 font-bold">Commit Orchestrator</span>
                  <p className="text-zinc-500 mt-1">All / Cluster / Pattern staging + exclude globs + GPG/SSH signing</p>
                </div>
              </div>
              <div className="text-center text-zinc-600">↓</div>

              {/* Layer 5: Web Dashboard */}
              <div className="flex items-center gap-4">
                <div className="flex-1 border border-blue-500/20 rounded-lg p-4 bg-blue-500/5 text-center">
                  <span className="text-blue-400 font-bold">Web Dashboard (SaaS)</span>
                  <p className="text-zinc-500 mt-1">Audit traces, team telemetry, policy management, billing</p>
                </div>
              </div>
            </div>
          </div>

          {/* Detailed Sections */}
          <div className="mt-24 space-y-16 max-w-5xl mx-auto">
            {/* Daemon */}
            <section className="grid gap-8 lg:grid-cols-2 lg:items-center">
              <div>
                <h2 className="text-2xl font-bold text-zinc-100">1. Daemon (Local Watcher)</h2>
                <p className="mt-4 text-zinc-400 leading-relaxed">
                  The daemon is a Rust-native binary that uses OS-level filesystem APIs (inotify on Linux, FSEvents on macOS) to watch repository trees. It converts raw OS events into structured <code className="bg-zinc-900 px-1 py-0.5 rounded text-yellow-400 font-mono">FsEvent</code> values and pushes them across a Tokio MPSC channel to the scheduler.
                </p>
                <p className="mt-4 text-zinc-400 leading-relaxed">
                  Ignore rules are loaded from <code className="bg-zinc-900 px-1 py-0.5 rounded text-yellow-400 font-mono">.kaptainignore</code> and support both exact paths and glob patterns. The watcher is recursive by default and synchronized with a readiness channel so startup failures surface immediately.
                </p>
              </div>
              <div className="border border-zinc-800 rounded-xl p-6 bg-zinc-900/60 font-mono text-xs text-zinc-400">
                <div className="text-zinc-500 mb-2"># watcher startup flow</div>
                <div><span className="text-violet-400">notify</span>::recommended_watcher()</div>
                <div className="pl-4">→ FsEvent {'{'} path, kind, timestamp {'}'}</div>
                <div className="pl-4">→ <span className="text-violet-400">blocking_send</span>(tx, event)</div>
                <div className="pl-4">→ scheduler receives batch</div>
                <div className="mt-4 text-zinc-500"># ignore matching</div>
                <div>globset::GlobSet::is_match(path)</div>
                <div>OR exact_relative_path_match(path)</div>
              </div>
            </section>

            {/* Cluster Engine */}
            <section className="grid gap-8 lg:grid-cols-2 lg:items-center">
              <div className="order-2 lg:order-1 border border-zinc-800 rounded-xl p-6 bg-zinc-900/60 font-mono text-xs text-zinc-400">
                <div className="text-zinc-500 mb-2"># cluster eligibility rule</div>
                <div>delta = event.time - last_event.time</div>
                <div>if delta &lt; window_seconds:</div>
                <div className="pl-4">cluster.push(event)</div>
                <div>else:</div>
                <div className="pl-4">flush(cluster) → analyze()</div>
                <div className="mt-4 text-zinc-500"># default config</div>
                <div>window = 5s</div>
                <div>min_commit_interval = 10s</div>
              </div>
              <div className="order-1 lg:order-2">
                <h2 className="text-2xl font-bold text-zinc-100">2. Cluster Engine</h2>
                <p className="mt-4 text-zinc-400 leading-relaxed">
                  The <code className="bg-zinc-900 px-1 py-0.5 rounded text-yellow-400 font-mono">ClusterEngine</code> groups filesystem events into logical change sets using a temporal sliding window. Events are added to the current cluster only while the time delta between consecutive events is strictly less than the configured window.
                </p>
                <p className="mt-4 text-zinc-400 leading-relaxed">
                  This transforms noisy, rapid-fire save events into cohesive units of work that map to developer intent. A minimum commit interval (default 10s) prevents excessive commits during burst editing.
                </p>
              </div>
            </section>

            {/* Diff Analysis */}
            <section>
              <h2 className="text-2xl font-bold text-zinc-100 text-center mb-8">3. Diff Analysis (Five Dimensions)</h2>
              <div className="grid gap-6 sm:grid-cols-2 lg:grid-cols-3">
                {dimensions.map((d) => (
                  <div
                    key={d.name}
                    className="border border-zinc-800 rounded-xl p-6 bg-zinc-900/60 hover:border-zinc-700 transition-all"
                  >
                    <div className="flex items-center justify-between mb-3">
                      <h3 className="text-lg font-semibold text-zinc-100">{d.name}</h3>
                      <span className="text-xs font-mono text-violet-400 bg-violet-400/10 px-2 py-1 rounded ring-1 ring-inset ring-violet-400/20">
                        {d.weight}
                      </span>
                    </div>
                    <p className="text-sm text-zinc-400 leading-relaxed">{d.desc}</p>
                  </div>
                ))}
                <div className="border border-zinc-800 rounded-xl p-6 bg-zinc-900/60 sm:col-span-2 lg:col-span-3">
                  <h3 className="text-lg font-semibold text-zinc-100 mb-2">Language Adapter Framework</h3>
                  <p className="text-sm text-zinc-400 leading-relaxed">
                    The <code className="bg-zinc-900 px-1 py-0.5 rounded text-yellow-400 font-mono">AdapterRegistry</code> resolves file paths to language-specific adapters. Supported languages: Rust, Go, Swift, Kotlin, TypeScript, JavaScript, Vue, Svelte, Astro, SCSS, HTML/CSS, and Python. Confidence is normalized per language: Rust/Go/Swift/Kotlin = 1.0, TypeScript = 0.9, Vue/Svelte/Astro = 0.85, Python = 0.8, JavaScript = 0.7, SCSS = 0.5, HTML/CSS = 0.4.
                  </p>
                </div>
              </div>
            </section>

            {/* Version Semver */}
            <section className="grid gap-8 lg:grid-cols-2 lg:items-center">
              <div>
                <h2 className="text-2xl font-bold text-zinc-100">4. Version Semver</h2>
                <p className="mt-4 text-zinc-400 leading-relaxed">
                  The version engine consumes the weighted composite score and applies deterministic rules:
                </p>
                <ul className="mt-4 space-y-2 text-sm text-zinc-400">
                  <li className="flex items-start gap-2">
                    <span className="text-red-400 font-bold">●</span>
                    <span><strong className="text-zinc-200">Breaking API detected</strong> → Major bump</span>
                  </li>
                  <li className="flex items-start gap-2">
                    <span className="text-violet-400 font-bold">●</span>
                    <span><strong className="text-zinc-200">Added API or score &gt; 0.6</strong> → Minor bump</span>
                  </li>
                  <li className="flex items-start gap-2">
                    <span className="text-emerald-400 font-bold">●</span>
                    <span><strong className="text-zinc-200">Score &gt; 0.1</strong> → Patch bump</span>
                  </li>
                  <li className="flex items-start gap-2">
                    <span className="text-zinc-500 font-bold">●</span>
                    <span><strong className="text-zinc-200">Otherwise</strong> → No bump</span>
                  </li>
                </ul>
                <p className="mt-4 text-zinc-400 leading-relaxed">
                  <code className="bg-zinc-900 px-1 py-0.5 rounded text-yellow-400 font-mono">save_version()</code> writes the <code className="bg-zinc-900 px-1 py-0.5 rounded text-yellow-400 font-mono">VERSION</code> file and also updates the version field in <code className="bg-zinc-900 px-1 py-0.5 rounded text-yellow-400 font-mono">Cargo.toml</code> when present.
                </p>
              </div>
              <div className="border border-zinc-800 rounded-xl p-6 bg-zinc-900/60 font-mono text-xs text-zinc-400">
                <div className="text-zinc-500 mb-2"># semver decision tree</div>
                <div>if breaking_api:</div>
                <div className="pl-4 text-red-400">bump = Major</div>
                <div>elif added_api || score &gt; 0.6:</div>
                <div className="pl-4 text-violet-400">bump = Minor</div>
                <div>elif score &gt; 0.1:</div>
                <div className="pl-4 text-emerald-400">bump = Patch</div>
                <div>else:</div>
                <div className="pl-4 text-zinc-500">bump = None</div>
                <div className="mt-4 text-zinc-500"># weight formula</div>
                <div>score = s*structural + a*api + d*deps + r*runtime + b*bundle</div>
              </div>
            </section>

            {/* Commit Orchestrator */}
            <section className="grid gap-8 lg:grid-cols-2 lg:items-center">
              <div className="order-2 lg:order-1 border border-zinc-800 rounded-xl p-6 bg-zinc-900/60 font-mono text-xs text-zinc-400">
                <div className="text-zinc-500 mb-2"># staging modes</div>
                <div><span className="text-violet-400">all</span>: index.add_all([&quot;*&quot;])</div>
                <div className="pl-4">→ remove(exclude_patterns)</div>
                <div><span className="text-violet-400">cluster</span>: stage(cluster.files)</div>
                <div className="pl-4">→ + VERSION + Cargo.toml</div>
                <div><span className="text-violet-400">pattern</span>: stage(include_globs)</div>
                <div className="pl-4">→ remove(exclude_patterns)</div>
                <div className="mt-4 text-zinc-500"># commit message includes</div>
                <div>bump, version, api_summary, files, score, cluster_id</div>
              </div>
              <div className="order-1 lg:order-2">
                <h2 className="text-2xl font-bold text-zinc-100">5. Commit Orchestrator</h2>
                <p className="mt-4 text-zinc-400 leading-relaxed">
                  The orchestrator supports three staging modes configured via <code className="bg-zinc-900 px-1 py-0.5 rounded text-yellow-400 font-mono">StagingConfig</code>:
                </p>
                <ul className="mt-4 space-y-2 text-sm text-zinc-400">
                  <li className="flex items-start gap-2">
                    <span className="text-violet-400 font-bold">●</span>
                    <span><strong className="text-zinc-200">All</strong> (default): stages everything, then removes exclude patterns.</span>
                  </li>
                  <li className="flex items-start gap-2">
                    <span className="text-violet-400 font-bold">●</span>
                    <span><strong className="text-zinc-200">Cluster</strong>: only stages files from the detected cluster plus VERSION and Cargo.toml.</span>
                  </li>
                  <li className="flex items-start gap-2">
                    <span className="text-violet-400 font-bold">●</span>
                    <span><strong className="text-zinc-200">Pattern</strong>: stages files matching include globs, then removes exclude patterns.</span>
                  </li>
                </ul>
                <p className="mt-4 text-zinc-400 leading-relaxed">
                  Commits are skipped when <code className="bg-zinc-900 px-1 py-0.5 rounded text-yellow-400 font-mono">Repo::is_clean()</code> reports no changes. Push is disabled by default and targets <code className="bg-zinc-900 px-1 py-0.5 rounded text-yellow-400 font-mono">refs/heads/&lt;branch&gt;</code> on origin when enabled.
                </p>
              </div>
            </section>

            {/* Web Dashboard */}
            <section className="border border-zinc-800 rounded-2xl p-8 bg-zinc-900/40">
              <div className="grid gap-8 lg:grid-cols-2 lg:items-center">
                <div>
                  <h2 className="text-2xl font-bold text-zinc-100">6. Web Dashboard</h2>
                  <p className="mt-4 text-zinc-400 leading-relaxed">
                    While the daemon runs locally, the Kaptaind Pro web dashboard provides centralized visibility into release decisions across teams and repositories.
                  </p>
                  <ul className="mt-4 space-y-2 text-sm text-zinc-400">
                    <li className="flex items-start gap-2">
                      <span className="text-blue-400 font-bold">●</span>
                      <span><strong className="text-zinc-200">Release Audit:</strong> Query every version decision, its composite score, and the cluster that triggered it.</span>
                    </li>
                    <li className="flex items-start gap-2">
                      <span className="text-blue-400 font-bold">●</span>
                      <span><strong className="text-zinc-200">AoC Traces:</strong> Browse Aim of Change sessions linked to agent model outputs and rationale.</span>
                    </li>
                    <li className="flex items-start gap-2">
                      <span className="text-blue-400 font-bold">●</span>
                      <span><strong className="text-zinc-200">Telemetry:</strong> Track token usage, inference costs, and build timing across the organization.</span>
                    </li>
                  </ul>
                </div>
                <div className="rounded-xl border border-zinc-800 bg-zinc-950 p-6 space-y-4">
                  <div className="flex items-center justify-between">
                    <span className="text-xs font-mono text-blue-400 uppercase">Dashboard Modules</span>
                    <span className="inline-flex items-center gap-1.5 rounded-full bg-green-400/10 px-2 py-0.5 text-xs font-medium text-green-400">
                      <span className="h-1.5 w-1.5 rounded-full bg-green-400 animate-ping" />
                      Live
                    </span>
                  </div>
                  <div className="space-y-2">
                    {["Release Governance", "Changelog Explorer", "Bump Reasoning", "Team Billing", "Policy Management"].map((m) => (
                      <div key={m} className="flex items-center justify-between border border-zinc-800 rounded-lg px-3 py-2 bg-zinc-900/50">
                        <span className="text-sm text-zinc-300">{m}</span>
                        <span className="text-xs text-zinc-500">→</span>
                      </div>
                    ))}
                  </div>
                </div>
              </div>
            </section>
          </div>

          {/* Whitepapers Link */}
          <div className="mt-24 rounded-2xl border border-zinc-800 bg-zinc-900/30 p-8 max-w-4xl mx-auto text-center">
            <h3 className="text-xl font-bold text-zinc-200 mb-2">
              Validated by Empirical Research
            </h3>
            <p className="text-sm text-zinc-400 leading-relaxed max-w-2xl mx-auto mb-6">
              Every architectural claim on this page has been tested and documented in our whitepapers. Read the full validation suite covering clustering, scoring, semver decisions, and commit orchestration.
            </p>
            <div className="flex justify-center gap-4">
              <Link
                href="/whitepapers"
                className="text-xs font-semibold bg-violet-600 hover:bg-violet-500 text-white py-2.5 px-4 rounded-lg"
              >
                Browse Whitepapers →
              </Link>
              <Link
                href="/download"
                className="text-xs font-semibold bg-zinc-800 hover:bg-zinc-700 text-zinc-200 py-2.5 px-4 rounded-lg"
              >
                Install the Daemon
              </Link>
            </div>
          </div>
        </div>
      </div>
      <Footer />
    </>
  );
}
