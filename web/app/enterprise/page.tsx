import Navbar from "@/components/layout/Navbar";
import Footer from "@/components/layout/Footer";
import EnterpriseContactForm from "@/components/EnterpriseContactForm";

export const metadata = {
  title: "Kaptaind | Enterprise Release Governance Platform",
  description: "Scale-out release automation with SAML SSO, RBAC, air-gapped support, custom retention, advanced policy packages, and secure audit exports.",
};

const valueProps = [
  {
    title: "SAML SSO & SCIM Syncing",
    desc: "Hook into your existing identity provider (Okta, Azure AD, Ping, Google) to provision users, enforce MFA, and revoke access instantly.",
  },
  {
    title: "Air-Gapped & Private Deployments",
    desc: "Run Kaptaind completely isolated inside your corporate VPC. We support external Postgres databases and standard S3-compatible object storage with no outbound traffic.",
  },
  {
    title: "Advanced Policy Engine",
    desc: "Enforce security standards across all repos. Set mandatory test coverage thresholds, ban specific dependencies, and restrict version promotions dynamically.",
  },
  {
    title: "Cryptographic Audit Logs",
    desc: "Export cryptographically-chained logs mapping every automated commit, version decision, and user override to a secure system record.",
  },
];

export default function EnterprisePage() {
  return (
    <>
      <Navbar />
      <div className="bg-zinc-950 text-zinc-100 min-h-screen py-24 sm:py-32">
        <div className="mx-auto max-w-7xl px-6 lg:px-8">
          <div className="mx-auto max-w-4xl text-center">
            <h1 className="text-4xl font-extrabold tracking-tight sm:text-6xl bg-gradient-to-r from-zinc-100 to-zinc-400 bg-clip-text text-transparent">
              Autonomous Release Governance for Regulated Industries
            </h1>
            <p className="mt-6 text-lg text-zinc-400 max-w-2xl mx-auto">
              Compliance-focused organizations choose Kaptaind to audit release pathways, enforce delivery policies, and eliminate manual coordination overhead.
            </p>
          </div>

          {/* Core Values Grid */}
          <div className="mt-20 grid gap-8 sm:grid-cols-2 max-w-5xl mx-auto">
            {valueProps.map((prop) => (
              <div key={prop.title} className="border border-zinc-800 rounded-xl p-6 bg-zinc-900/40">
                <h3 className="text-lg font-semibold text-zinc-200 mb-2">{prop.title}</h3>
                <p className="text-sm text-zinc-400 leading-relaxed">{prop.desc}</p>
              </div>
            ))}
          </div>

          {/* Demo Contact / CTA Form */}
          <div className="mt-24 max-w-2xl mx-auto border border-zinc-800 rounded-2xl p-8 bg-zinc-900/60 backdrop-blur-sm">
            <h3 className="text-xl font-bold text-center text-zinc-100 mb-2">Request an Enterprise Evaluation</h3>
            <p className="text-xs text-zinc-500 text-center mb-6">
              Get in touch with our team for custom pricing, security packages, and architecture reviews.
            </p>
            <EnterpriseContactForm />
          </div>
        </div>
      </div>
      <Footer />
    </>
  );
}
