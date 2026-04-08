'use client';

import Link from "next/link";
import { useTheme } from "@/components/providers/ThemeProvider";

export default function Navbar() {
  const { theme, toggleTheme } = useTheme();

  return (
    <nav className="sticky top-0 z-50 border-b border-zinc-800 bg-zinc-950/80 backdrop-blur-md">
      <div className="mx-auto flex h-16 max-w-7xl items-center justify-between px-4 sm:px-6 lg:px-8">
        <Link href="/" className="flex items-center gap-2">
          <div className="flex h-8 w-8 items-center justify-center rounded-lg bg-violet-600 text-sm font-bold text-white">
            K
          </div>
          <span className="text-lg font-bold text-zinc-100">
            Kaptaind<span className="text-violet-500">Pro</span>
          </span>
        </Link>

        <div className="hidden items-center gap-6 sm:flex">
          <Link
            href="/#features"
            className="text-sm text-zinc-400 hover:text-zinc-100 transition-colors"
          >
            Features
          </Link>
          <Link
            href="/pricing"
            className="text-sm text-zinc-400 hover:text-zinc-100 transition-colors"
          >
            Pricing
          </Link>
          <Link
            href="/auth/signin"
            className="text-sm text-zinc-400 hover:text-zinc-100 transition-colors"
          >
            Sign In
          </Link>

          <button
            onClick={toggleTheme}
            className="rounded-lg p-2 text-zinc-400 hover:bg-zinc-800 hover:text-zinc-100 transition-colors"
            title={`Switch to ${theme === 'dark' ? 'light' : 'dark'} mode`}
          >
            {theme === 'dark' ? (
              <svg className="h-5 w-5" fill="currentColor" viewBox="0 0 20 20">
                <path d="M17.293 13.293A8 8 0 016.707 2.707a8.001 8.001 0 1010.586 10.586z" />
              </svg>
            ) : (
              <svg className="h-5 w-5" fill="currentColor" viewBox="0 0 20 20">
                <path
                  fillRule="evenodd"
                  d="M10 2a1 1 0 011 1v1a1 1 0 11-2 0V3a1 1 0 011-1zm4.293 2.293a1 1 0 011.414 0l.707.707a1 1 0 11-1.414 1.414l-.707-.707a1 1 0 010-1.414zm2.828 2.828a1 1 0 011.414 0l.707.707a1 1 0 11-1.414 1.414l-.707-.707a1 1 0 010-1.414zM10 7a3 3 0 110 6 3 3 0 010-6zm0 1a2 2 0 100 4 2 2 0 000-4zm4.293 1.293a1 1 0 011.414 0l.707.707a1 1 0 11-1.414 1.414l-.707-.707a1 1 0 010-1.414zM3 13a1 1 0 011 1v1a1 1 0 11-2 0v-1a1 1 0 011-1zm1.293-2.293a1 1 0 011.414 0l.707.707a1 1 0 11-1.414 1.414l-.707-.707a1 1 0 010-1.414zM3 17a1 1 0 011 1v1a1 1 0 11-2 0v-1a1 1 0 011-1zm10-1a1 1 0 01.707 1.707l-.707.707a1 1 0 11-1.414-1.414l.707-.707a1 1 0 01.707-.293z"
                  clipRule="evenodd"
                />
              </svg>
            )}
          </button>

          <Link
            href="/auth/signup"
            className="rounded-lg bg-violet-600 px-4 py-2 text-sm font-medium text-white hover:bg-violet-700 transition-colors"
          >
            Get Started
          </Link>
        </div>
      </div>
    </nav>
  );
}
