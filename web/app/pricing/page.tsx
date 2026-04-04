import Link from "next/link";
import Navbar from "@/components/layout/Navbar";
import Footer from "@/components/layout/Footer";

const plans = [
  {
    name: "Free",
    price: "$0",
    period: "forever",
    description: "For individual developers getting started with Kaptaind.",
    features: [
      "Daemon status monitoring",
      "Analysis history log",
      "Version tracking",
      "AoC session management",
    ],
    cta: "Get Started",
    ctaHref: "/auth/signup",
    highlight: false,
  },
  {
    name: "Pro",
    price: "$19",
    period: "/month",
    description:
      "For teams who want AI-powered insights into their versioning workflow.",
    features: [
      "Everything in Free",
      "AI commit messages (Claude)",
      "Changelog generation from AoC",
      "Smart bump reasoning",
      "Team dashboard & analytics",
      "Priority support",
    ],
    cta: "Start Pro Trial",
    ctaHref: "/auth/signup",
    highlight: true,
  },
];

export default function PricingPage() {
  return (
    <>
      <Navbar />
      <div className="mx-auto max-w-5xl px-4 py-24 sm:px-6 lg:px-8">
        <div className="text-center">
          <h1 className="text-4xl font-bold text-zinc-900 dark:text-zinc-100">
            Simple, transparent pricing
          </h1>
          <p className="mt-4 text-lg text-zinc-600 dark:text-zinc-400">
            Start free. Upgrade when you need AI.
          </p>
        </div>

        <div className="mt-16 grid gap-8 sm:grid-cols-2">
          {plans.map((plan) => (
            <div
              key={plan.name}
              className={`rounded-2xl border p-8 ${
                plan.highlight
                  ? "border-violet-500 bg-white shadow-lg shadow-violet-100 dark:bg-zinc-900 dark:shadow-violet-900/20"
                  : "border-zinc-200 bg-white dark:border-zinc-800 dark:bg-zinc-900"
              }`}
            >
              {plan.highlight && (
                <div className="mb-4 inline-block rounded-full bg-violet-100 px-3 py-1 text-xs font-medium text-violet-700 dark:bg-violet-900/30 dark:text-violet-400">
                  Most Popular
                </div>
              )}
              <h2 className="text-2xl font-bold text-zinc-900 dark:text-zinc-100">
                {plan.name}
              </h2>
              <div className="mt-2 flex items-baseline gap-1">
                <span className="text-4xl font-bold text-zinc-900 dark:text-zinc-100">
                  {plan.price}
                </span>
                <span className="text-sm text-zinc-500">{plan.period}</span>
              </div>
              <p className="mt-4 text-sm text-zinc-600 dark:text-zinc-400">
                {plan.description}
              </p>

              <ul className="mt-8 space-y-3">
                {plan.features.map((feature) => (
                  <li
                    key={feature}
                    className="flex items-center gap-2 text-sm text-zinc-700 dark:text-zinc-300"
                  >
                    <svg
                      className="h-4 w-4 flex-shrink-0 text-violet-500"
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
                    {feature}
                  </li>
                ))}
              </ul>

              <Link
                href={plan.ctaHref}
                className={`mt-8 block w-full rounded-lg px-4 py-2.5 text-center text-sm font-medium ${
                  plan.highlight
                    ? "bg-violet-600 text-white hover:bg-violet-700"
                    : "border border-zinc-300 text-zinc-700 hover:bg-zinc-50 dark:border-zinc-700 dark:text-zinc-300 dark:hover:bg-zinc-800"
                }`}
              >
                {plan.cta}
              </Link>
            </div>
          ))}
        </div>
      </div>
      <Footer />
    </>
  );
}
