import Link from "next/link";
import Navbar from "@/components/layout/Navbar";
import Footer from "@/components/layout/Footer";

export const metadata = {
  title: "Kaptaind | Customer Case Studies",
  description: "Read how modern engineering organizations leverage Kaptaind's autonomous release governance to scale delivery velocity, enforce policies, and reduce release coordination overhead.",
};

const caseStudies = [
  {
    company: "Linear FinTech",
    scale: "180+ Engineers",
    metric: "90% Reduction in Lead Time for Changes",
    description: "Operating in a highly regulated banking space, Linear required strict version auditing and manual release checklists. By adopting Kaptaind, they moved to policy-gated, AST-aware automated release decisions, saving 15 engineering hours per week.",
    outcome: "Ensured compliance with SOC2 release audit rules using cryptographically-chained trace manifests.",
  },
  {
    company: "SaaS Analytics Inc",
    scale: "60+ Developers",
    metric: "12x Increase in Deployment Frequency",
    description: "SaaS Analytics struggled with release bottlenecks. Developers frequently missed breaking API changes, leading to broken downstream pipelines. Kaptaind's AST analysis automatically flags public API modifications and triggers minor version bumps dynamically.",
    outcome: "Reduced main branch regression incidents by 40% using pre-commit policy enforcement.",
  },
  {
    company: "CloudCore Infrastructure",
    scale: "300+ Engineers",
    metric: "$140K Annual Saved in AI Token Fees",
    description: "CloudCore wanted to automate commit descriptions using LLMs but faced massive API token bills. By upgrading to Kaptaind Enterprise, they enabled Request Batching and Smart Caching, slashing their inference costs by 64% while maintaining narrative quality.",
    outcome: "Standardized release metadata across 140 microservices with zero developer friction.",
  },
];

export default function CaseStudiesPage() {
  return (
    <>
      <Navbar />
      <div className="bg-zinc-950 text-zinc-100 min-h-screen py-24 sm:py-32">
        <div className="mx-auto max-w-5xl px-6 lg:px-8">
          <div className="text-center">
            <h1 className="text-4xl font-extrabold tracking-tight sm:text-5xl">
              Customer Success Stories
            </h1>
            <p className="mt-4 text-lg text-zinc-400">
              See how platform engineering teams drive release velocity and compliance using Kaptaind.
            </p>
          </div>

          <div className="mt-16 space-y-12">
            {caseStudies.map((cs) => (
              <div key={cs.company} className="border border-zinc-800 rounded-2xl p-8 bg-zinc-900/40 hover:border-zinc-700 transition-all">
                <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between border-b border-zinc-800/60 pb-4 mb-4">
                  <div>
                    <h3 className="text-2xl font-bold text-zinc-100">{cs.company}</h3>
                    <span className="text-xs text-zinc-500 font-mono">{cs.scale}</span>
                  </div>
                  <div className="mt-2 sm:mt-0">
                    <span className="inline-flex items-center rounded-md bg-violet-400/10 px-2.5 py-1 text-xs font-semibold text-violet-400 ring-1 ring-inset ring-violet-400/20">
                      {cs.metric}
                    </span>
                  </div>
                </div>
                <p className="text-sm text-zinc-400 leading-relaxed mb-4">
                  {cs.description}
                </p>
                <div className="bg-zinc-950/40 p-4 rounded-lg border border-zinc-800/40 text-xs">
                  <strong className="text-zinc-200">Governance Outcome:</strong> {cs.outcome}
                </div>
              </div>
            ))}
          </div>

          <div className="mt-24 text-center rounded-2xl border border-zinc-800 bg-zinc-900/20 p-8">
            <h3 className="text-xl font-bold mb-2">Ready to achieve similar outcomes?</h3>
            <p className="text-sm text-zinc-400 mb-6 max-w-md mx-auto">Get in touch with our team for a tailored release governance pilot for your organization.</p>
            <Link href="/enterprise" className="bg-violet-600 hover:bg-violet-500 text-white font-semibold py-3 px-6 rounded-lg shadow-lg shadow-violet-600/20 transition-all text-sm">
              Request Pilot Demo
            </Link>
          </div>
        </div>
      </div>
      <Footer />
    </>
  );
}
