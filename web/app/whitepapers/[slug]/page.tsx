import { notFound } from "next/navigation";
import { readFile, readdir } from "fs/promises";
import { join } from "path";
import Navbar from "@/components/layout/Navbar";
import Footer from "@/components/layout/Footer";
import MarkdownRenderer from "@/components/MarkdownRenderer";
import Link from "next/link";

interface Props {
  params: Promise<{ slug: string }>;
}

export async function generateStaticParams() {
  const files = await readdir(join(process.cwd(), "public", "whitepapers"));
  return files
    .filter((f) => f.endsWith(".md"))
    .map((f) => ({ slug: f.replace(/\.md$/, "") }));
}

export async function generateMetadata({ params }: Props) {
  const { slug } = await params;
  const title = slug
    .split("-")
    .map((w) => w.charAt(0).toUpperCase() + w.slice(1))
    .join(" ");
  return {
    title: `Kaptaind | Whitepaper: ${title}`,
  };
}

export default async function WhitepaperPage({ params }: Props) {
  const { slug } = await params;
  const filePath = join(process.cwd(), "public", "whitepapers", `${slug}.md`);

  let content: string;
  try {
    content = await readFile(filePath, "utf-8");
  } catch {
    notFound();
  }

  return (
    <>
      <Navbar />
      <div className="bg-zinc-950 text-zinc-100 min-h-screen py-12">
        <div className="mx-auto max-w-3xl px-6 lg:px-8">
          <Link
            href="/whitepapers"
            className="inline-flex items-center text-sm text-zinc-500 hover:text-zinc-300 transition-colors mb-8"
          >
            ← Back to Research & Validation
          </Link>
          <article className="prose prose-invert max-w-none">
            <MarkdownRenderer content={content} />
          </article>
          <div className="mt-16 pt-8 border-t border-zinc-800 text-center">
            <Link
              href="/whitepapers"
              className="inline-flex items-center text-sm text-zinc-500 hover:text-zinc-300 transition-colors"
            >
              ← Browse all whitepapers
            </Link>
          </div>
        </div>
      </div>
      <Footer />
    </>
  );
}
