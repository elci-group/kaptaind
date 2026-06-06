import Navbar from "@/components/layout/Navbar";
import Footer from "@/components/layout/Footer";

export const metadata = {
  title: "Kaptaind | Documentation",
  description: "Explore the comprehensive guides, API scoring frameworks, policy engine rules, and integration guides for Kaptaind.",
};

const docArticles = [
  {
    category: "Getting Started",
    title: "Quick Start Guide",
    anchor: "quickstart",
    body: "To get started with Kaptaind, install the CLI daemon on your machine. Run `kaptaind init` inside your Git repository root to initialize the `kaptaind.toml` config file. The daemon will automatically detect changes, verify your test suites, and write semantic release commits.",
  },
  {
    category: "Core Engine",
    title: "Change Clustering",
    anchor: "clustering",
    body: "Kaptaind batches filesystem events using a temporal sliding window. Instead of creating a commit for every single file save, it groups files that were saved in proximity (default: 5 seconds) into logical commits matching your single intent.",
  },
  {
    category: "Scoring Engine",
    title: "Multi-Dimensional Score Analysis",
    anchor: "scoring",
    body: "Changes are analyzed on 4 distinct dimensions: 1) Structural (event density and churn), 2) AST (analyzes whether modified symbols are part of the public API surface), 3) Dependency manifest changes, and 4) Runtime configurations (Vercel configs, Dockerfiles, Kubernetes manifests). Weights are compiled to determine semantic bumps.",
  },
  {
    category: "Policy Engine",
    title: "Defining Release Policies",
    anchor: "policies",
    body: "Policies define criteria for automated commits and pushes. Set `minimum_tests = 'cargo test'` to block commits if tests fail. Use `disallowed_file_patterns` to prevent committing secrets or forbidden extensions, and configure version bump qualifiers.",
  },
];

export default function DocsPage() {
  return (
    <>
      <Navbar />
      <div className="bg-zinc-950 text-zinc-100 min-h-screen py-24 sm:py-32 font-sans">
        <div className="mx-auto max-w-7xl px-6 lg:px-8">
          <div className="flex flex-col md:flex-row gap-12">
            {/* Sidebar Navigation */}
            <aside className="w-full md:w-64 flex-shrink-0">
              <div className="sticky top-28 space-y-6">
                <div>
                  <h3 className="text-xs font-semibold text-zinc-500 uppercase tracking-wider mb-3">Release Docs</h3>
                  <ul className="space-y-2 text-sm text-zinc-400">
                    <li>
                      <a href="#quickstart" className="hover:text-violet-400 block py-1">Quick Start Guide</a>
                    </li>
                    <li>
                      <a href="#clustering" className="hover:text-violet-400 block py-1">Change Clustering</a>
                    </li>
                    <li>
                      <a href="#scoring" className="hover:text-violet-400 block py-1">Multi-Dimensional Analysis</a>
                    </li>
                    <li>
                      <a href="#policies" className="hover:text-violet-400 block py-1">Release Policies</a>
                    </li>
                  </ul>
                </div>
              </div>
            </aside>

            {/* Main Content Area */}
            <main className="flex-1 max-w-3xl">
              {/* Docs Search Mock */}
              <div className="mb-12 relative rounded-lg border border-zinc-800 bg-zinc-900/60 p-4">
                <label className="block text-xs font-semibold text-zinc-400 mb-2">Search Documentation</label>
                <div className="relative">
                  <input
                    type="text"
                    placeholder="Search terms (e.g. policy packs, custom hooks)..."
                    className="w-full bg-zinc-950 border border-zinc-800 rounded-lg py-2.5 pl-10 pr-4 text-sm text-zinc-300 focus:outline-none focus:border-violet-500"
                  />
                  <span className="absolute left-3.5 top-3 text-zinc-500">🔍</span>
                </div>
              </div>

              {/* Doc Articles */}
              <div className="space-y-16">
                {docArticles.map((art) => (
                  <article key={art.anchor} id={art.anchor} className="scroll-mt-28">
                    <span className="text-xs font-mono text-violet-400 uppercase tracking-wider block mb-1">{art.category}</span>
                    <h2 className="text-2xl font-bold text-zinc-100 mb-4 border-b border-zinc-900 pb-2">{art.title}</h2>
                    <p className="text-sm text-zinc-400 leading-relaxed">{art.body}</p>
                  </article>
                ))}
              </div>
            </main>
          </div>
        </div>
      </div>
      <Footer />
    </>
  );
}
