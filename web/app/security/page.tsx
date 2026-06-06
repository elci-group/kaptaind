import Navbar from "@/components/layout/Navbar";
import Footer from "@/components/layout/Footer";

export const metadata = {
  title: "Kaptaind | Security, Trust & Compliance Posture",
  description: "Learn how Kaptaind aligns with NIST SP 800-218 Secure Software Development Framework (SSDF) and OWASP Application Security Verification Standard (ASVS) v4.0 to secure release governance.",
};

const ssdfPractices = [
  {
    category: "Prepare the Organization (PO)",
    standards: "NIST SSDF PO.1, PO.2",
    details: "All administrative actions, policy adjustments, and trace accesses are cataloged in our structured audit engine. Role-Based Access Control (RBAC) separates duties between developers and release compliance administrators.",
  },
  {
    category: "Protect the Software (PS)",
    standards: "NIST SSDF PS.1, PS.2",
    details: "Kaptaind binaries and desktop installers (macOS Developer ID, Windows MSIX, Linux AppImage) are cryptographically signed. Updates enforce signature validation and rollback prevention to defend against supply chain tampering.",
  },
  {
    category: "Produce Secure Software (PW)",
    standards: "NIST SSDF PW.1, PW.4",
    details: "We scan dependencies at build time, evaluate AST changes, and enforce pre-commit testing gates. Vulnerabilities in dependency trees are flagged in our API scoring dimensions.",
  },
  {
    category: "Respond to Vulnerabilities (RV)",
    standards: "NIST SSDF RV.1, RV.3",
    details: "We monitor open vulnerabilities and issue rapid patch updates. Enterprise contracts include dedicated SLA-backed patches for security disclosures.",
  },
];

const asvsControls = [
  {
    domain: "V2: Authentication Verification Requirements",
    alignment: "OWASP ASVS Level 3",
    details: "Leverages NextAuth with robust provider boundaries. Native credentials use cryptographically strong salt-and-hash algorithms (bcryptjs). Multi-factor authentication (MFA) is delegated to identity providers (SAML/OIDC).",
  },
  {
    domain: "V3: Session Management Verification",
    alignment: "OWASP ASVS Level 3",
    details: "Tokens are cryptographically signed and stored in HTTP-only, secure, same-site cookies. Active sessions are automatically revoked upon token expiration or remote logout.",
  },
  {
    domain: "V4: Access Control Verification",
    alignment: "OWASP ASVS Level 3",
    details: "We execute strict server-side validation. Entitlements block unauthorized features (such as SSO or Audit Exports) in the application layer, ignoring client-side parameters.",
  },
  {
    domain: "V5: Validation, Sanitization and Encoding",
    alignment: "OWASP ASVS Level 3",
    details: "Our server-side analytics engine sanitizes all telemetry. Sensitive variables, repository path hierarchies, and git commit details are hashed and redacted before transit, leaving zero corporate metadata footprints.",
  },
];

export default function SecurityPage() {
  return (
    <>
      <Navbar />
      <div className="bg-zinc-950 text-zinc-100 min-h-screen py-24 sm:py-32">
        <div className="mx-auto max-w-7xl px-6 lg:px-8">
          <div className="mx-auto max-w-4xl text-center">
            <h1 className="text-4xl font-extrabold tracking-tight sm:text-5xl">
              Security, Compliance & Trust
            </h1>
            <p className="mt-4 text-lg text-zinc-400">
              Kaptaind secures automated release boundaries using enterprise-grade controls mapped to globally recognized frameworks.
            </p>
          </div>

          {/* Intro Box */}
          <div className="mt-16 border border-zinc-800 rounded-2xl p-8 bg-zinc-900/30 max-w-4xl mx-auto text-sm leading-relaxed text-zinc-300">
            <h3 className="text-lg font-semibold text-zinc-200 mb-2">Our Core Posture</h3>
            <p className="mb-4">
              Autonomous release governance requires rigorous verification. Kaptaind does not require push access to your core source code in the cloud tier. Diffs are scored locally within your trusted execution context by the Rust daemon. Only aggregate compliance signals and release decisions are uploaded to the dashboard.
            </p>
            <p>
              For regulated industries, Kaptaind supports an <strong>Enterprise Self-Hosted Deployment Mode</strong> which operates entirely within your VPC (connected to external Postgres and object storage) with <strong>no outbound telemetry by default</strong>.
            </p>
          </div>

          {/* NIST SSDF Section */}
          <div className="mt-24">
            <div className="text-center mb-12">
              <h2 className="text-2xl font-bold text-zinc-100">NIST SSDF SP 800-218 Alignment</h2>
              <p className="mt-2 text-sm text-zinc-400">
                How our development and deployment pipelines map to the Secure Software Development Framework (SP 800-218).
              </p>
            </div>

            <div className="grid gap-8 sm:grid-cols-2 max-w-5xl mx-auto">
              {ssdfPractices.map((practice) => (
                <div key={practice.category} className="border border-zinc-800 rounded-xl p-6 bg-zinc-900/60">
                  <div className="flex items-center justify-between mb-2">
                    <h3 className="font-semibold text-zinc-200">{practice.category}</h3>
                    <span className="text-xs font-mono bg-violet-900/30 text-violet-400 px-2 py-0.5 rounded">{practice.standards}</span>
                  </div>
                  <p className="text-sm text-zinc-400 leading-relaxed">{practice.details}</p>
                </div>
              ))}
            </div>
          </div>

          {/* OWASP ASVS Section */}
          <div className="mt-24">
            <div className="text-center mb-12">
              <h2 className="text-2xl font-bold text-zinc-100">OWASP ASVS V4.0 Compliance</h2>
              <p className="mt-2 text-sm text-zinc-400">
                Application controls aligned to the Application Security Verification Standard verification goals.
              </p>
            </div>

            <div className="grid gap-8 sm:grid-cols-2 max-w-5xl mx-auto">
              {asvsControls.map((ctrl) => (
                <div key={ctrl.domain} className="border border-zinc-800 rounded-xl p-6 bg-zinc-900/60">
                  <div className="flex items-center justify-between mb-2">
                    <h3 className="font-semibold text-zinc-200 text-sm">{ctrl.domain}</h3>
                    <span className="text-xs font-mono bg-blue-900/30 text-blue-400 px-2 py-0.5 rounded">{ctrl.alignment}</span>
                  </div>
                  <p className="text-sm text-zinc-400 leading-relaxed">{ctrl.details}</p>
                </div>
              ))}
            </div>
          </div>

          {/* Signatures & Updates */}
          <div className="mt-24 border border-zinc-800 rounded-2xl p-8 max-w-4xl mx-auto bg-zinc-900/30">
            <h3 className="text-xl font-bold text-zinc-200 mb-4 text-center">Binary Integrity & Signing Rules</h3>
            <div className="grid gap-6 sm:grid-cols-3 text-center text-xs">
              <div className="p-4 border border-zinc-800 bg-zinc-950 rounded-lg">
                <span className="font-semibold text-zinc-200 block mb-1">macOS Distribution</span>
                <p className="text-zinc-500">Developer ID signing, hardened runtime enabled, and notarized under Gatekeeper standards.</p>
              </div>
              <div className="p-4 border border-zinc-800 bg-zinc-950 rounded-lg">
                <span className="font-semibold text-zinc-200 block mb-1">Windows Distribution</span>
                <p className="text-zinc-500">Production MSIX/NSIS installers signed using trusted Code Signing certificates.</p>
              </div>
              <div className="p-4 border border-zinc-800 bg-zinc-950 rounded-lg">
                <span className="font-semibold text-zinc-200 block mb-1">Auto-Update Policy</span>
                <p className="text-zinc-500">Updates are signed cryptographically, with rollback prevention. Disabled on self-hosted enterprise unless enabled by admins.</p>
              </div>
            </div>
          </div>
        </div>
      </div>
      <Footer />
    </>
  );
}
