import Link from "next/link";
import Navbar from "@/components/layout/Navbar";
import Footer from "@/components/layout/Footer";

export const metadata = {
  title: "Kaptaind | Research & Validation",
  description:
    "Empirical whitepapers validating every claim made about Kaptaind's daemon, scoring engine, and enterprise features.",
};

const whitepapers = [
  {
    slug: "clustering",
    title: "Event Clustering by Temporal Proximity",
    result: "PASS",
    desc: "Validates that filesystem events are grouped into clusters based on configurable time windows.",
  },
  {
    slug: "five-dimensions",
    title: "Five-Dimensional Impact Scoring",
    result: "PASS",
    desc: "Validates that diff analysis produces structural, API, dependency, runtime, and bundle scores.",
  },
  {
    slug: "semver-decisions",
    title: "Semantic Versioning Decision Engine",
    result: "PASS",
    desc: "Validates deterministic semver bump rules driven by API changes and composite scores.",
  },
  {
    slug: "language-adapters",
    title: "Language Adapter Coverage",
    result: "PASS",
    desc: "Validates 12 language adapters and public API symbol extraction for Rust and TypeScript.",
  },
  {
    slug: "commit-staging",
    title: "Commit Orchestrator & Staging Modes",
    result: "PASS",
    desc: "Validates All, Cluster, and Pattern staging modes with exclude-glob support.",
  },
  {
    slug: "analysis-artifacts",
    title: "Analysis Artifact Integrity",
    result: "PASS",
    desc: "Validates that persisted JSON artifacts contain all fields required to explain a bump.",
  },
  {
    slug: "local-first",
    title: "Local-First Architecture",
    result: "PASS",
    desc: "Validates that default configuration requires no external APIs, webhooks, or cloud services.",
  },
  {
    slug: "test-hooks",
    title: "Test Hook Compliance",
    result: "PASS",
    desc: "Validates that required test hooks block commits and optional hooks do not.",
  },
  {
    slug: "push-gate",
    title: "Push Gate Behavior",
    result: "PASS",
    desc: "Validates that push is disabled by default and correctly gated by configuration.",
  },
  {
    slug: "saas-gap-analysis",
    title: "SaaS & Enterprise Claims — Gap Analysis",
    result: "PARTIAL",
    desc: "Honest audit of SSO, RBAC, audit trails, retention, self-hosting, and policy engine claims.",
  },
];

export default function WhitepapersPage() {
  return (
    <>
      <Navbar />
      <div className="bg-zinc-950 text-zinc-100 min-h-screen py-24">
        <div className="mx-auto max-w-5xl px-6 lg:px-8">
          <div className="mx-auto max-w-3xl text-center mb-16">
            <h1 className="text-4xl font-extrabold tracking-tight sm:text-5xl">
              Research & Validation
            </h1>
            <p className="mt-4 text-lg text-zinc-400">
              Every claim on our landing page has been tested and documented.
              No marketing fluff — just empirical results.
            </p>
          </div>

          <div className="space-y-4">
            {whitepapers.map((paper) => (
              <Link
                key={paper.slug}
                href={`/whitepapers/${paper.slug}`}
                className="flex items-center justify-between rounded-xl border border-zinc-800 bg-zinc-900/40 p-6 hover:border-zinc-600 hover:bg-zinc-900/60 transition-all"
              >
                <div className="flex-1">
                  <div className="flex items-center gap-3 mb-2">
                    <h3 className="text-lg font-semibold text-zinc-100">
                      {paper.title}
                    </h3>
                    <span
                      className={`inline-flex items-center rounded-full px-2.5 py-0.5 text-xs font-medium ${
                        paper.result === "PASS"
                          ? "bg-emerald-500/10 text-emerald-400 ring-1 ring-inset ring-emerald-500/20"
                          : "bg-amber-500/10 text-amber-400 ring-1 ring-inset ring-amber-500/20"
                      }`}
                    >
                      {paper.result}
                    </span>
                  </div>
                  <p className="text-sm text-zinc-400">{paper.desc}</p>
                </div>
                <span className="ml-4 text-zinc-500 text-lg">→</span>
              </Link>
            ))}
          </div>

          <div className="mt-16 rounded-xl border border-zinc-800 bg-zinc-900/30 p-8">
            <h2 className="text-xl font-bold text-zinc-100 mb-4">Test Suite Summary</h2>
            <div className="grid grid-cols-3 gap-6 text-center">
              <div>
                <div className="text-3xl font-extrabold text-emerald-400">9</div>
                <div className="text-xs text-zinc-500 mt-1 uppercase tracking-wider">Claims Fully Supported</div>
              </div>
              <div>
                <div className="text-3xl font-extrabold text-amber-400">1</div>
                <div className="text-xs text-zinc-500 mt-1 uppercase tracking-wider">Partial / Gap Identified</div>
              </div>
              <div>
                <div className="text-3xl font-extrabold text-zinc-100">18</div>
                <div className="text-xs text-zinc-500 mt-1 uppercase tracking-wider">Integration Tests Run</div>
              </div>
            </div>
            <p className="mt-6 text-xs text-zinc-500 text-center font-mono">
              Test suite: tests/claims_validation.rs • Run with: cargo test --test claims_validation
            </p>
          </div>
        </div>
      </div>
      <Footer />
    </>
  );
}
