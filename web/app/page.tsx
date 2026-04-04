import Link from "next/link";
import Navbar from "@/components/layout/Navbar";
import Footer from "@/components/layout/Footer";

const features = [
  {
    title: "AI Commit Messages",
    description:
      "Transform mechanical version bumps into rich, narrative commit descriptions powered by Claude.",
    icon: "&#x1f4dd;",
  },
  {
    title: "Changelog Generation",
    description:
      "Generate polished markdown changelogs from your Aim of Change sessions with a single click.",
    icon: "&#x1f4cb;",
  },
  {
    title: "Smart Bump Reasoning",
    description:
      "Understand why each semver decision was made with plain-English explanations from the LLM.",
    icon: "&#x1f9e0;",
  },
  {
    title: "Team Dashboard",
    description:
      "Track commit velocity, token usage, and team activity across your projects.",
    icon: "&#x1f4ca;",
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
      <section className="relative overflow-hidden bg-white dark:bg-zinc-950">
        <div className="mx-auto max-w-7xl px-4 py-24 sm:px-6 lg:px-8 lg:py-32">
          <div className="text-center">
            <h1 className="text-4xl font-bold tracking-tight text-zinc-900 dark:text-zinc-100 sm:text-6xl">
              Semantic Versioning,
              <br />
              <span className="text-violet-600">Powered by AI</span>
            </h1>
            <p className="mx-auto mt-6 max-w-2xl text-lg text-zinc-600 dark:text-zinc-400">
              Kaptaind watches your repository, analyzes every change, and
              automatically bumps your version. Pro unlocks AI-powered commit
              messages, changelogs, and team analytics.
            </p>
            <div className="mt-10 flex items-center justify-center gap-4">
              <Link
                href="/auth/signup"
                className="rounded-lg bg-violet-600 px-6 py-3 text-sm font-medium text-white hover:bg-violet-700"
              >
                Get Started Free
              </Link>
              <Link
                href="/pricing"
                className="rounded-lg border border-zinc-300 px-6 py-3 text-sm font-medium text-zinc-700 hover:bg-zinc-50 dark:border-zinc-700 dark:text-zinc-300 dark:hover:bg-zinc-900"
              >
                View Pricing
              </Link>
            </div>
          </div>

          {/* Terminal mockup */}
          <div className="mx-auto mt-16 max-w-2xl overflow-hidden rounded-xl border border-zinc-200 bg-zinc-950 shadow-2xl dark:border-zinc-800">
            <div className="flex items-center gap-2 border-b border-zinc-800 bg-zinc-900 px-4 py-3">
              <div className="h-3 w-3 rounded-full bg-red-500" />
              <div className="h-3 w-3 rounded-full bg-yellow-500" />
              <div className="h-3 w-3 rounded-full bg-green-500" />
              <span className="ml-2 text-xs text-zinc-500">kaptaind</span>
            </div>
            <pre className="overflow-x-auto p-4 font-mono text-sm leading-6 text-green-400">
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
      <section className="border-t border-zinc-200 bg-zinc-50 dark:border-zinc-800 dark:bg-zinc-900">
        <div className="mx-auto max-w-7xl px-4 py-24 sm:px-6 lg:px-8">
          <h2 className="text-center text-3xl font-bold text-zinc-900 dark:text-zinc-100">
            How It Works
          </h2>
          <div className="mt-16 grid gap-8 sm:grid-cols-3">
            {steps.map((step) => (
              <div key={step.num} className="text-center">
                <div className="mx-auto flex h-12 w-12 items-center justify-center rounded-full bg-violet-600 text-lg font-bold text-white">
                  {step.num}
                </div>
                <h3 className="mt-4 text-lg font-semibold text-zinc-900 dark:text-zinc-100">
                  {step.title}
                </h3>
                <p className="mt-2 text-sm text-zinc-600 dark:text-zinc-400">
                  {step.description}
                </p>
              </div>
            ))}
          </div>
        </div>
      </section>

      {/* Features */}
      <section
        id="features"
        className="border-t border-zinc-200 dark:border-zinc-800"
      >
        <div className="mx-auto max-w-7xl px-4 py-24 sm:px-6 lg:px-8">
          <h2 className="text-center text-3xl font-bold text-zinc-900 dark:text-zinc-100">
            Pro Features
          </h2>
          <p className="mx-auto mt-4 max-w-2xl text-center text-zinc-600 dark:text-zinc-400">
            Unlock the full power of Kaptaind with AI-enhanced insights.
          </p>
          <div className="mt-16 grid gap-8 sm:grid-cols-2">
            {features.map((f) => (
              <div
                key={f.title}
                className="rounded-xl border border-zinc-200 bg-white p-6 dark:border-zinc-800 dark:bg-zinc-900"
              >
                <div
                  className="mb-3 text-3xl"
                  dangerouslySetInnerHTML={{ __html: f.icon }}
                />
                <h3 className="text-lg font-semibold text-zinc-900 dark:text-zinc-100">
                  {f.title}
                </h3>
                <p className="mt-2 text-sm text-zinc-600 dark:text-zinc-400">
                  {f.description}
                </p>
              </div>
            ))}
          </div>
        </div>
      </section>

      {/* CTA */}
      <section className="border-t border-zinc-200 bg-violet-600 dark:border-zinc-800">
        <div className="mx-auto max-w-7xl px-4 py-16 text-center sm:px-6 lg:px-8">
          <h2 className="text-3xl font-bold text-white">
            Ready to automate your versioning?
          </h2>
          <p className="mx-auto mt-4 max-w-xl text-violet-100">
            Start free and upgrade when you need AI-powered insights.
          </p>
          <Link
            href="/auth/signup"
            className="mt-8 inline-block rounded-lg bg-white px-6 py-3 text-sm font-medium text-violet-600 hover:bg-zinc-50"
          >
            Get Started
          </Link>
        </div>
      </section>

      <Footer />
    </>
  );
}
