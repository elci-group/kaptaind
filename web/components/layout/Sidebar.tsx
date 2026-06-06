"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import { signOut } from "next-auth/react";
import Badge from "@/components/ui/Badge";

const navItems = [
  { href: "/dashboard", label: "Overview", pro: false },
  { href: "/dashboard/ai-commits", label: "AI Commits", pro: true },
  { href: "/dashboard/changelog", label: "Changelog", pro: true },
  { href: "/dashboard/bump-reasoning", label: "Bump Reasoning", pro: true },
  { href: "/dashboard/team", label: "Team", pro: true },
  { href: "/dashboard/policies", label: "Policies", pro: true },
  { href: "/dashboard/audit", label: "Audit Log", pro: true },
];

export default function Sidebar({ tier }: { tier: "free" | "pro" | "team" | "enterprise" }) {
  const pathname = usePathname();

  return (
    <aside className="flex h-full w-64 flex-col border-r border-zinc-200 bg-zinc-50 dark:border-zinc-800 dark:bg-zinc-950">
      {/* Logo */}
      <div className="flex h-16 items-center gap-2 border-b border-zinc-200 px-6 dark:border-zinc-800">
        <div className="flex h-8 w-8 items-center justify-center rounded-lg bg-violet-600 text-sm font-bold text-white">
          K
        </div>
        <span className="text-lg font-bold text-zinc-900 dark:text-zinc-100">
          Kaptaind<span className="text-violet-600">Pro</span>
        </span>
      </div>

      {/* Navigation */}
      <nav className="flex-1 space-y-1 px-3 py-4">
        {navItems.map((item) => {
          const isActive =
            item.href === "/dashboard"
              ? pathname === "/dashboard"
              : pathname.startsWith(item.href);

          return (
            <Link
              key={item.href}
              href={item.href}
              className={`flex items-center justify-between rounded-lg px-3 py-2 text-sm font-medium transition-colors ${
                isActive
                  ? "bg-violet-100 text-violet-900 dark:bg-violet-900/20 dark:text-violet-300"
                  : "text-zinc-600 hover:bg-zinc-100 hover:text-zinc-900 dark:text-zinc-400 dark:hover:bg-zinc-800 dark:hover:text-zinc-100"
              }`}
            >
              <span>{item.label}</span>
              {item.pro && <Badge variant="pro">Pro</Badge>}
            </Link>
          );
        })}
      </nav>

      {/* Subscription & Sign Out */}
      <div className="border-t border-zinc-200 p-4 dark:border-zinc-800">
        <div className="mb-3 flex items-center justify-between">
          <span className="text-xs text-zinc-500 dark:text-zinc-400">Plan</span>
          <Badge variant={tier === "free" ? "default" : "pro"}>
            {tier === "free" ? "Free" : tier.charAt(0).toUpperCase() + tier.slice(1)}
          </Badge>
        </div>
        {tier === "free" && (
          <Link
            href="/api/billing/create-checkout?plan=pro"
            className="mb-3 block w-full rounded-lg bg-violet-600 px-3 py-2 text-center text-sm font-medium text-white hover:bg-violet-700"
          >
            Upgrade to Pro
          </Link>
        )}
        <button
          onClick={() => signOut({ callbackUrl: "/" })}
          className="w-full rounded-lg px-3 py-2 text-sm text-zinc-500 hover:bg-zinc-100 hover:text-zinc-700 dark:text-zinc-400 dark:hover:bg-zinc-800 dark:hover:text-zinc-200"
        >
          Sign Out
        </button>
      </div>
    </aside>
  );
}
