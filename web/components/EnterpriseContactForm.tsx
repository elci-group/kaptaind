"use client";

export default function EnterpriseContactForm() {
  return (
    <form className="space-y-4 text-sm" onSubmit={(e) => e.preventDefault()}>
      <div className="grid gap-4 sm:grid-cols-2">
        <div>
          <label className="block text-xs font-semibold text-zinc-400 mb-1">First & Last Name</label>
          <input type="text" placeholder="Jane Doe" className="w-full bg-zinc-950 border border-zinc-800 rounded-lg p-2.5 text-zinc-300 focus:outline-none focus:border-violet-500" required />
        </div>
        <div>
          <label className="block text-xs font-semibold text-zinc-400 mb-1">Work Email</label>
          <input type="email" placeholder="jane@company.com" className="w-full bg-zinc-950 border border-zinc-800 rounded-lg p-2.5 text-zinc-300 focus:outline-none focus:border-violet-500" required />
        </div>
      </div>
      <div>
        <label className="block text-xs font-semibold text-zinc-400 mb-1">Company / Organization</label>
        <input type="text" placeholder="Acme Corp" className="w-full bg-zinc-950 border border-zinc-800 rounded-lg p-2.5 text-zinc-300 focus:outline-none focus:border-violet-500" required />
      </div>
      <div>
        <label className="block text-xs font-semibold text-zinc-400 mb-1">Deploy Mode Preference</label>
        <select className="w-full bg-zinc-950 border border-zinc-800 rounded-lg p-2.5 text-zinc-400 focus:outline-none focus:border-violet-500">
          <option>Managed SaaS (Single-Tenant Isolation)</option>
          <option>Self-Hosted (VPC / On-Premises)</option>
          <option>Air-Gapped / Sovereign Cloud</option>
        </select>
      </div>
      <div>
        <label className="block text-xs font-semibold text-zinc-400 mb-1">Requirements or Scope</label>
        <textarea placeholder="Tell us about your team size, repository volume, and compliance requirements..." rows={3} className="w-full bg-zinc-950 border border-zinc-800 rounded-lg p-2.5 text-zinc-300 focus:outline-none focus:border-violet-500" />
      </div>
      <button type="submit" className="w-full bg-violet-600 hover:bg-violet-500 text-white font-semibold py-3 px-4 rounded-lg shadow-lg shadow-violet-600/20 transition-all mt-4">
        Schedule Demo / Call
      </button>
    </form>
  );
}
