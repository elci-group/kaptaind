import Navbar from "@/components/layout/Navbar";
import Footer from "@/components/layout/Footer";
import CopyButton from "@/components/CopyButton";

export const metadata = {
  title: "Kaptaind | Download Local CLI & Desktop App",
  description: "Get the local-first Kaptaind release daemon CLI or the official signed desktop control plane for macOS, Windows, and Linux.",
};

const desktopDownloads = [
  {
    os: "macOS (Apple Silicon & Intel)",
    format: "DMG (Signed & Notarized)",
    desc: "Signed with Developer ID, optimized for Gatekeeper compliance on Apple macOS.",
    link: "#",
    btnText: "Download for Mac",
  },
  {
    os: "Windows 10 / 11",
    format: "MSIX Installer (Signed)",
    desc: "Fully signed Windows Package for smooth installation under SmartScreen verification.",
    link: "#",
    btnText: "Download for Windows",
  },
  {
    os: "Linux (Debian / Fedora / Ubuntu)",
    format: "AppImage / DEB / RPM",
    desc: "Standalone AppImage binary executable or platform package bundles.",
    link: "#",
    btnText: "Download for Linux",
  },
];

export default function DownloadPage() {
  return (
    <>
      <Navbar />
      <div className="bg-zinc-950 text-zinc-100 min-h-screen py-24 sm:py-32">
        <div className="mx-auto max-w-5xl px-6 lg:px-8">
          <div className="text-center">
            <h1 className="text-4xl font-extrabold tracking-tight sm:text-5xl">
              Download Kaptaind Tools
            </h1>
            <p className="mt-4 text-lg text-zinc-400">
              Install the open-source CLI daemon locally or run the official desktop governance dashboard.
            </p>
          </div>

          {/* CLI Section */}
          <div className="mt-16 border border-zinc-800 rounded-2xl p-8 bg-zinc-900/30 max-w-3xl mx-auto">
            <h2 className="text-2xl font-bold text-zinc-100 mb-2">1. Install the Local CLI Daemon</h2>
            <p className="text-sm text-zinc-400 mb-6">
              Run this single shell command to automatically download the correct binary, configure system paths, and register the daemon service.
            </p>
            <div className="bg-zinc-950 p-4 rounded-lg border border-zinc-800 font-mono text-sm text-violet-400 flex items-center justify-between overflow-x-auto">
              <code>curl -fsSL https://get.kaptaind.com/install.sh | sh</code>
              <CopyButton text="curl -fsSL https://get.kaptaind.com/install.sh | sh" />
            </div>
            <p className="text-xs text-zinc-500 mt-2">
              Supports macOS (Darwin), Linux (glibc/musl), and Windows (WSL/native bash). Read our installation docs for manual binary placement options.
            </p>
          </div>

          {/* Desktop App Section */}
          <div className="mt-24">
            <h2 className="text-2xl font-bold text-center text-zinc-100 mb-2">2. Install the Tauri Desktop Dashboard</h2>
            <p className="text-sm text-zinc-400 text-center max-w-lg mx-auto mb-12">
              The Tauri desktop control plane lets you manage local watched repositories, track release qualification status, and sync traces with the cloud console.
            </p>

            <div className="grid gap-8 sm:grid-cols-3">
              {desktopDownloads.map((dl) => (
                <div key={dl.os} className="border border-zinc-800 rounded-xl p-6 bg-zinc-900/60 flex flex-col justify-between">
                  <div>
                    <h3 className="font-bold text-zinc-100 text-lg mb-1">{dl.os}</h3>
                    <span className="text-xs font-mono text-violet-400 block mb-3">{dl.format}</span>
                    <p className="text-xs text-zinc-400 leading-relaxed mb-6">{dl.desc}</p>
                  </div>
                  <button className="w-full bg-zinc-800 hover:bg-zinc-700 text-zinc-200 text-xs font-semibold py-2.5 rounded-lg active:scale-95 transition-all">
                    {dl.btnText}
                  </button>
                </div>
              ))}
            </div>
          </div>

          {/* Verification section */}
          <div className="mt-20 border-t border-zinc-800 pt-12 text-center text-xs text-zinc-500 max-w-lg mx-auto">
            <h3 className="font-bold text-zinc-400 mb-2">Verifying Downloads</h3>
            <p className="leading-relaxed">
              All production binaries and updates are cryptographically signed. You can verify public keys or review build hashes (SHA-256) inside our secure repository changelogs before installation.
            </p>
          </div>
        </div>
      </div>
      <Footer />
    </>
  );
}
