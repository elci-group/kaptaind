import Link from "next/link";
import Navbar from "@/components/layout/Navbar";
import Footer from "@/components/layout/Footer";

const coreFeatures = [
  {
    title: "Automatic Versioning",
    description: "Daemon continuously monitors your repo and computes semantic version bumps deterministically.",
    icon: "⚙️",
  },
  {
    title: "Multi-Dimensional Analysis",
    description:
      "Analyzes API, dependencies, runtime, and code structure to determine precise version impacts.",
    icon: "🔍",
  },
  {
    title: "Aim of Change Sessions",
    description:
      "Group related commits into intent-driven sessions with full traceability and audit trails.",
    icon: "🎯",
  },
  {
    title: "100% Open Source",
    description:
      "Self-hosted, no cloud dependency. Full control over your versioning pipeline.",
    icon: "🔓",
  },
];

const aiFeatures = [
  {
    title: "AI Commit Messages",
    description:
      "Transform mechanical version bumps into rich, narrative commit descriptions powered by Claude.",
    icon: "📝",
  },
  {
    title: "Changelog Generation",
    description:
      "Generate polished markdown changelogs from your Aim of Change sessions with a single click.",
    icon: "📋",
  },
  {
    title: "Smart Bump Reasoning",
    description:
      "Understand why each semver decision was made with plain-English explanations.",
    icon: "🧠",
  },
  {
    title: "Team Analytics",
    description:
      "Track commit velocity, token usage, costs, and team activity across projects.",
    icon: "📊",
  },
];

const enterpriseFeatures = [
  {
    title: "Intelligent Request Batching",
    description: "Automatically batch inference requests to reduce API calls by 40-60% without quality loss.",
    icon: "⚡",
  },
  {
    title: "Smart Caching & Memoization",
    description: "Cache parsed results and reuse inference for similar code patterns across commits.",
    icon: "💾",
  },
  {
    title: "Dynamic Model Selection",
    description: "Automatically choose cheaper models for routine tasks, expensive ones for complex analysis.",
    icon: "🎛️",
  },
  {
    title: "Cost Monitoring & Alerts",
    description: "Real-time tracking of inference costs with configurable alerts and spend caps.",
    icon: "💰",
  },
];

const steps = [
  {
    num: "1",
    title: "Watch",
    description: "The daemon monitors your repository for file changes in real time.",
  },
  {
    num: "2",
    title: "Analyze",
    description:
      "Diffs are scored across structural, API, dependency, and runtime dimensions.",
  },
  {
    num: "3",
    title: "Commit",
    description:
      "A semantic version bump is decided, VERSION is updated, and a commit is created automatically.",
  },
];

export default function LandingPage() {
  return (
    <>
      <Navbar />

      {/* Hero */}
      <section className="relative overflow-hidden bg-zinc-950">
        <div className="mx-auto max-w-7xl px-4 py-24 sm:px-6 lg:px-8 lg:py-32">
          <div className="text-center">
            <div className="mb-4 flex justify-center gap-2">
              <span className="rounded-full bg-green-900/30 px-3 py-1 text-xs font-medium text-green-400">
                100% Open Source
              </span>
              <span className="rounded-full bg-blue-900/30 px-3 py-1 text-xs font-medium text-blue-400">
                SaaS Pro+Enterprise
              </span>
            </div>
            <h1 className="text-4xl font-bold tracking-tight text-zinc-100 sm:text-6xl">
              Deterministic Semantic Versioning
              <br />
              <span className="bg-gradient-to-r from-violet-500 to-purple-500 bg-clip-text text-transparent">at Scale</span>
            </h1>
            <p className="mx-auto mt-6 max-w-2xl text-lg text-zinc-400">
              Kaptaind is a self-governing release system that eliminates manual version bumping.
              Start free with our open-source daemon. Add AI-powered insights and cost optimization with Pro or Enterprise.
            </p>
            <div className="mt-10 flex items-center justify-center gap-4">
              <Link
                href="/auth/signup"
                className="rounded-lg bg-violet-600 px-6 py-3 text-sm font-medium text-white hover:bg-violet-700 transition-colors"
              >
                Start Free (OSS)
              </Link>
              <Link
                href="https://github.com/elci-group/kaptaind"
                className="rounded-lg border border-zinc-700 px-6 py-3 text-sm font-medium text-zinc-300 hover:bg-zinc-900 transition-colors"
              >
                GitHub Repo
              </Link>
            </div>
          </div>

          {/* Terminal mockup */}
          <div className="mx-auto mt-16 max-w-2xl overflow-hidden rounded-xl border border-zinc-800 bg-zinc-900 shadow-2xl">
            <div className="flex items-center gap-2 border-b border-zinc-800 bg-zinc-800 px-4 py-3">
              <div className="h-3 w-3 rounded-full bg-red-500" />
              <div className="h-3 w-3 rounded-full bg-yellow-500" />
              <div className="h-3 w-3 rounded-full bg-green-500" />
              <span className="ml-2 text-xs text-zinc-400">kaptaind</span>
            </div>
            <pre className="overflow-x-auto p-4 font-mono text-sm leading-6 text-emerald-400">
              <code>{`$ kaptaind-cli status
🚢 Kaptaind Status
=================
📂 Repository:  /home/dev/my-project
🏷️  Version:     0.3.12
⚙️  Daemon:      🟢 Running

$ kaptaind-cli log --limit 3
┌──────────┬──────────┬───────┬───────┐
│ Version  │ Bump     │ Score │ Paths │
├──────────┼──────────┼───────┼───────┤
│ v0.3.12  │ 🩹 Patch │ 0.156 │     4 │
│ v0.3.11  │ 🩹 Patch │ 0.152 │     3 │
│ v0.3.10  │ ✨ Minor │ 0.620 │     7 │
└──────────┴──────────┴───────┴───────┘`}</code>
            </pre>
          </div>
        </div>
      </section>

      {/* How It Works */}
      <section className="border-t border-zinc-800 bg-zinc-900">
        <div className="mx-auto max-w-7xl px-4 py-24 sm:px-6 lg:px-8">
          <h2 className="text-center text-3xl font-bold text-zinc-100">
            How It Works
          </h2>
          <div className="mt-16 grid gap-8 sm:grid-cols-3">
            {steps.map((step) => (
              <div key={step.num} className="text-center">
                <div className="mx-auto flex h-12 w-12 items-center justify-center rounded-full bg-gradient-to-br from-violet-600 to-purple-600 text-lg font-bold text-white">
                  {step.num}
                </div>
                <h3 className="mt-4 text-lg font-semibold text-zinc-100">
                  {step.title}
                </h3>
                <p className="mt-2 text-sm text-zinc-400">
                  {step.description}
                </p>
              </div>
            ))}
          </div>
        </div>
      </section>

      {/* Core Features (For Everyone) */}
      <section id="features" className="border-t border-zinc-800 bg-zinc-950">
        <div className="mx-auto max-w-7xl px-4 py-24 sm:px-6 lg:px-8">
          <div className="mb-12">
            <div className="mx-auto mb-2 w-fit rounded-full bg-green-900/30 px-3 py-1 text-xs font-medium text-green-400">
              FOR EVERYONE
            </div>
            <h2 className="text-center text-3xl font-bold text-zinc-100">
              Powerful Core Features
            </h2>
            <p className="mx-auto mt-4 max-w-2xl text-center text-zinc-400">
              Everything you need for deterministic semantic versioning—open source, self-hosted, zero cloud dependency.
            </p>
          </div>
          <div className="mt-16 grid gap-8 sm:grid-cols-2">
            {coreFeatures.map((f) => (
              <div
                key={f.title}
                className="rounded-xl border border-zinc-800 bg-zinc-900 p-6 hover:border-zinc-700 hover:bg-zinc-800/50 transition-colors"
              >
                <div className="mb-3 text-4xl">{f.icon}</div>
                <h3 className="text-lg font-semibold text-zinc-100">
                  {f.title}
                </h3>
                <p className="mt-2 text-sm text-zinc-400">
                  {f.description}
                </p>
              </div>
            ))}
          </div>
        </div>
      </section>

      {/* Pro Features */}
      <section className="border-t border-zinc-800 bg-zinc-900">
        <div className="mx-auto max-w-7xl px-4 py-24 sm:px-6 lg:px-8">
          <div className="mb-12">
            <div className="mx-auto mb-2 w-fit rounded-full bg-blue-900/30 px-3 py-1 text-xs font-medium text-blue-400">
              FOR TEAMS
            </div>
            <h2 className="text-center text-3xl font-bold text-zinc-100">
              SaaS Pro: AI-Powered Insights
            </h2>
            <p className="mx-auto mt-4 max-w-2xl text-center text-zinc-400">
              Unlock Claude-powered insights, rich commit messages, automated changelogs, and team dashboards.
            </p>
          </div>
          <div className="mt-16 grid gap-8 sm:grid-cols-2">
            {aiFeatures.map((f) => (
              <div
                key={f.title}
                className="rounded-xl border border-zinc-800 bg-zinc-800/50 p-6 hover:border-blue-700/50 hover:bg-blue-900/10 transition-colors"
              >
                <div className="mb-3 text-4xl">{f.icon}</div>
                <h3 className="text-lg font-semibold text-zinc-100">
                  {f.title}
                </h3>
                <p className="mt-2 text-sm text-zinc-400">
                  {f.description}
                </p>
              </div>
            ))}
          </div>
          <div className="mt-12 flex justify-center">
            <Link
              href="/pricing"
              className="rounded-lg bg-blue-600 px-6 py-3 text-sm font-medium text-white hover:bg-blue-700 transition-colors"
            >
              View Pro Pricing
            </Link>
          </div>
        </div>
      </section>

      {/* Enterprise Features */}
      <section className="border-t border-zinc-800 bg-zinc-950">
        <div className="mx-auto max-w-7xl px-4 py-24 sm:px-6 lg:px-8">
          <div className="mb-12">
            <div className="mx-auto mb-2 w-fit rounded-full bg-purple-900/30 px-3 py-1 text-xs font-medium text-purple-400">
              FOR SCALE
            </div>
            <h2 className="text-center text-3xl font-bold text-zinc-100">
              Enterprise: Cost-Optimized Inference
            </h2>
            <p className="mx-auto mt-4 max-w-2xl text-center text-zinc-400">
              Advanced batching, caching, and dynamic model selection to reduce inference costs by 60%+ while maintaining quality.
            </p>
          </div>
          <div className="mt-16 grid gap-8 sm:grid-cols-2">
            {enterpriseFeatures.map((f) => (
              <div
                key={f.title}
                className="rounded-xl border border-zinc-800 bg-zinc-900 p-6 hover:border-purple-700/50 hover:bg-purple-900/10 transition-colors"
              >
                <div className="mb-3 text-4xl">{f.icon}</div>
                <h3 className="text-lg font-semibold text-zinc-100">
                  {f.title}
                </h3>
                <p className="mt-2 text-sm text-zinc-400">
                  {f.description}
                </p>
              </div>
            ))}
          </div>
          <div className="mt-12 flex justify-center">
            <Link
              href="/pricing"
              className="rounded-lg bg-purple-600 px-6 py-3 text-sm font-medium text-white hover:bg-purple-700 transition-colors"
            >
              Enterprise Pricing
            </Link>
          </div>
        </div>
      </section>

      {/* CTA */}
      <section className="border-t border-zinc-800 bg-gradient-to-b from-purple-950/50 via-zinc-950 to-zinc-950">
        <div className="mx-auto max-w-5xl px-4 py-16 sm:px-6 lg:px-8">
          <div className="text-center">
            <h2 className="text-3xl font-bold text-zinc-100">
              Choose your path to deterministic versioning
            </h2>
            <p className="mx-auto mt-4 max-w-xl text-zinc-400">
              Start free with our open-source daemon. Scale with Pro inference. Optimize costs with Enterprise.
            </p>
          </div>

          <div className="mt-12 grid gap-6 sm:grid-cols-3">
            {/* Free Card */}
            <div className="rounded-xl border border-zinc-700 bg-zinc-900 p-6 hover:border-green-700/50 hover:bg-green-900/5 transition-colors">
              <h3 className="text-lg font-semibold text-zinc-100">Free</h3>
              <p className="mt-2 text-sm text-zinc-400">Self-hosted daemon</p>
              <Link
                href="/auth/signup"
                className="mt-6 block rounded-lg bg-green-600 px-4 py-2 text-center text-sm font-medium text-white hover:bg-green-700 transition-colors"
              >
                Get Started
              </Link>
            </div>

            {/* Pro Card */}
            <div className="rounded-xl border border-blue-700/50 bg-blue-900/20 p-6 hover:bg-blue-900/30 transition-colors">
              <h3 className="text-lg font-semibold text-blue-100">Pro</h3>
              <p className="mt-2 text-sm text-blue-300">AI insights & analytics</p>
              <Link
                href="/pricing"
                className="mt-6 block rounded-lg bg-blue-600 px-4 py-2 text-center text-sm font-medium text-white hover:bg-blue-700 transition-colors"
              >
                View Pricing
              </Link>
            </div>

            {/* Enterprise Card */}
            <div className="rounded-xl border border-purple-700/50 bg-purple-900/20 p-6 hover:bg-purple-900/30 transition-colors">
              <h3 className="text-lg font-semibold text-purple-100">Enterprise</h3>
              <p className="mt-2 text-sm text-purple-300">Cost-optimized inference</p>
              <Link
                href="/pricing"
                className="mt-6 block rounded-lg bg-purple-600 px-4 py-2 text-center text-sm font-medium text-white hover:bg-purple-700 transition-colors"
              >
                View Pricing
              </Link>
            </div>
          </div>
        </div>
      </section>

      <Footer />
    </>
  );
}
