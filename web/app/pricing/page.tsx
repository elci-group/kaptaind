import Link from "next/link";
import Navbar from "@/components/layout/Navbar";
import Footer from "@/components/layout/Footer";

const plans = [
  {
    name: "Free",
    subtitle: "Self-Hosted",
    price: "$0",
    period: "forever",
    description: "Full-featured daemon for individuals and open-source projects.",
    tier: "free",
    features: [
      { name: "Deterministic semantic versioning", included: true },
      { name: "Multi-dimensional diff analysis", included: true },
      { name: "Aim of Change sessions", included: true },
      { name: "100% open source & self-hosted", included: true },
      { name: "CLI tools (kaptaind, kaptaind-cli)", included: true },
      { name: "AI commit messages", included: false },
      { name: "Changelog generation", included: false },
      { name: "Team dashboard", included: false },
      { name: "Cost monitoring", included: false },
    ],
    cta: "Get Started",
    ctaHref: "/auth/signup",
    highlight: false,
  },
  {
    name: "Pro",
    subtitle: "Cloud-Hosted SaaS",
    price: "$29",
    period: "/month",
    description: "AI-powered insights for teams with managed hosting and dashboard.",
    tier: "pro",
    features: [
      { name: "Everything in Free", included: true },
      { name: "Cloud-hosted daemon", included: true },
      { name: "AI commit messages (Claude 3.5 Sonnet)", included: true },
      { name: "Intelligent changelog generation", included: true },
      { name: "Smart bump reasoning", included: true },
      { name: "Team dashboard & analytics", included: true },
      { name: "Commit velocity tracking", included: true },
      { name: "Token usage analytics", included: true },
      { name: "Priority support", included: true },
      { name: "Cost optimization (Enterprise only)", included: false },
    ],
    cta: "Start Pro Trial",
    ctaHref: "/auth/signup",
    highlight: true,
  },
  {
    name: "Enterprise",
    subtitle: "Cost-Optimized Scale",
    price: "Custom",
    period: "contact sales",
    description: "Advanced inference optimization for high-volume releases.",
    tier: "enterprise",
    features: [
      { name: "Everything in Pro", included: true },
      { name: "Intelligent request batching (40-60% cost reduction)", included: true },
      { name: "Smart result caching & memoization", included: true },
      { name: "Dynamic model selection", included: true },
      { name: "Real-time cost monitoring & alerts", included: true },
      { name: "Spend cap enforcement", included: true },
      { name: "Custom inference thresholds", included: true },
      { name: "Dedicated account manager", included: true },
      { name: "SLA & uptime guarantees", included: true },
      { name: "Volume discounts on inference", included: true },
    ],
    cta: "Schedule Demo",
    ctaHref: "https://calendly.com/kaptaind",
    highlight: false,
  },
];

export default function PricingPage() {
  return (
    <>
      <Navbar />
      <div className="mx-auto max-w-7xl px-4 py-24 sm:px-6 lg:px-8 bg-zinc-950 min-h-screen">
        <div className="text-center">
          <h1 className="text-4xl font-bold text-zinc-100">
            Transparent pricing for every scale
          </h1>
          <p className="mt-4 text-lg text-zinc-400">
            Start free with our open-source daemon. Add AI and scale affordably with cost optimization.
          </p>
        </div>

        <div className="mt-16 grid gap-8 sm:grid-cols-3">
          {plans.map((plan) => (
            <div
              key={plan.name}
              className={`relative rounded-2xl border p-8 ${
                plan.highlight
                  ? "border-violet-500 bg-zinc-900 shadow-lg shadow-violet-900/20 lg:scale-105"
                  : "border-zinc-800 bg-zinc-900"
              }`}
            >
              {plan.highlight && (
                <div className="mb-4 inline-block rounded-full bg-violet-900/30 px-3 py-1 text-xs font-medium text-violet-400">
                  Most Popular
                </div>
              )}

              <div>
                <h2 className="text-2xl font-bold text-zinc-100">
                  {plan.name}
                </h2>
                <p className="mt-1 text-xs font-medium text-zinc-400">
                  {plan.subtitle}
                </p>
              </div>

              <div className="mt-6 flex items-baseline gap-1">
                {plan.price === "Custom" ? (
                  <>
                    <span className="text-4xl font-bold text-zinc-100">
                      Custom
                    </span>
                  </>
                ) : (
                  <>
                    <span className="text-4xl font-bold text-zinc-100">
                      {plan.price}
                    </span>
                    <span className="text-sm text-zinc-400">{plan.period}</span>
                  </>
                )}
              </div>

              <p className="mt-4 text-sm text-zinc-400">
                {plan.description}
              </p>

              <ul className="mt-8 space-y-3">
                {plan.features.map((feature) => (
                  <li
                    key={feature.name}
                    className={`flex items-start gap-2 text-sm ${
                      feature.included
                        ? "text-zinc-300"
                        : "text-zinc-500"
                    }`}
                  >
                    {feature.included ? (
                      <svg
                        className="h-4 w-4 flex-shrink-0 text-violet-500 mt-0.5"
                        fill="none"
                        viewBox="0 0 24 24"
                        strokeWidth={2}
                        stroke="currentColor"
                      >
                        <path
                          strokeLinecap="round"
                          strokeLinejoin="round"
                          d="M4.5 12.75l6 6 9-13.5"
                        />
                      </svg>
                    ) : (
                      <svg
                        className="h-4 w-4 flex-shrink-0 text-zinc-600 mt-0.5"
                        fill="none"
                        viewBox="0 0 24 24"
                        strokeWidth={2}
                        stroke="currentColor"
                      >
                        <path
                          strokeLinecap="round"
                          strokeLinejoin="round"
                          d="M6 18L18 6M6 6l12 12"
                        />
                      </svg>
                    )}
                    {feature.name}
                  </li>
                ))}
              </ul>

              <Link
                href={plan.ctaHref}
                className={`mt-8 block w-full rounded-lg px-4 py-2.5 text-center text-sm font-medium transition-colors ${
                  plan.highlight
                    ? "bg-violet-600 text-white hover:bg-violet-700"
                    : "border border-zinc-700 text-zinc-300 hover:bg-zinc-800"
                }`}
              >
                {plan.cta}
              </Link>
            </div>
          ))}
        </div>

        {/* Cost Comparison */}
        <div className="mt-24">
          <h2 className="text-center text-3xl font-bold text-zinc-100">
            Why Enterprise Saves Money
          </h2>
          <div className="mt-12 grid gap-8 sm:grid-cols-3">
            <div className="rounded-xl border border-zinc-800 bg-zinc-900 p-6">
              <h3 className="font-semibold text-zinc-100">
                Without Optimization
              </h3>
              <p className="mt-2 text-2xl font-bold text-red-500">$500/month</p>
              <p className="mt-2 text-sm text-zinc-400">
                Naive inference on every commit (100 commits/month, $5 per commit)
              </p>
            </div>

            <div className="rounded-xl border border-violet-700/50 bg-violet-900/20 p-6">
              <h3 className="font-semibold text-violet-100">
                With Enterprise Batching
              </h3>
              <p className="mt-2 text-2xl font-bold text-violet-400">$175/month</p>
              <p className="mt-2 text-sm text-violet-300">
                Smart batching + caching reduces effective cost by 65%
              </p>
            </div>

            <div className="rounded-xl border border-green-700/50 bg-green-900/20 p-6">
              <h3 className="font-semibold text-green-100">
                Monthly Savings
              </h3>
              <p className="mt-2 text-2xl font-bold text-green-400">$325/month</p>
              <p className="mt-2 text-sm text-green-300">
                ROI on Enterprise in just 1 month (if &gt;5 engineers)
              </p>
            </div>
          </div>
        </div>

        {/* FAQ */}
        <div className="mt-24">
          <h2 className="text-center text-3xl font-bold text-zinc-100">
            Frequently Asked Questions
          </h2>
          <div className="mt-12 space-y-6 max-w-3xl mx-auto">
            <div className="rounded-lg border border-zinc-800 bg-zinc-900 p-6">
              <h3 className="font-semibold text-zinc-100">
                Can I run Free tier in production?
              </h3>
              <p className="mt-2 text-sm text-zinc-400">
                Yes! The open-source daemon is production-ready with no vendor lock-in. Many teams run it at scale.
              </p>
            </div>
            <div className="rounded-lg border border-zinc-800 bg-zinc-900 p-6">
              <h3 className="font-semibold text-zinc-100">
                What&apos;s the difference between Pro and Enterprise?
              </h3>
              <p className="mt-2 text-sm text-zinc-400">
                Pro adds AI-powered insights. Enterprise adds cost optimization through batching, caching, and smart model selection—critical for high-volume teams.
              </p>
            </div>
            <div className="rounded-lg border border-zinc-800 bg-zinc-900 p-6">
              <h3 className="font-semibold text-zinc-100">
                Do you offer annual billing discounts?
              </h3>
              <p className="mt-2 text-sm text-zinc-400">
                Yes, both Pro and Enterprise offer 20% discounts for annual commitments. Contact sales for details.
              </p>
            </div>
            <div className="rounded-lg border border-zinc-800 bg-zinc-900 p-6">
              <h3 className="font-semibold text-zinc-100">
                Can I migrate from Free to Pro/Enterprise?
              </h3>
              <p className="mt-2 text-sm text-zinc-400">
                Absolutely. You can start free and upgrade anytime. Your data and configuration carry over seamlessly.
              </p>
            </div>
          </div>
        </div>
      </div>
      <Footer />
    </>
  );
}
