import { NextResponse } from "next/server";
import { getServerSession } from "next-auth";
import { authOptions } from "@/lib/auth";
import { resolveRepoPath } from "@/lib/kaptaind/reader";
import { getAnalysisArtifact } from "@/lib/kaptaind/analysis";
import { listAocManifests } from "@/lib/kaptaind/aoc";
import { inferenceChat } from "@/lib/inference";
import type { AnalysisArtifact, AocManifest } from "@/types/kaptaind";

export async function POST(req: Request) {
  const session = await getServerSession(authOptions);
  if (!session?.user) {
    return NextResponse.json({ error: "Unauthorized" }, { status: 401 });
  }

  try {
    const body = await req.json();
    const { projectId, aocId } = body;

    if (!projectId || !aocId) {
      return NextResponse.json(
        { error: "projectId and aocId required" },
        { status: 400 }
      );
    }

    const repoPath = await resolveRepoPath(projectId);
    const manifests = await listAocManifests(repoPath, 100);
    const manifest = manifests.find((m) => m.id === aocId);

    if (!manifest) {
      return NextResponse.json(
        { error: "AoC manifest not found" },
        { status: 404 }
      );
    }

    const changelog = await generateChangelogFromManifest(
      repoPath,
      manifest
    );
    return NextResponse.json({ changelog });
  } catch (error) {
    const message =
      error instanceof Error ? error.message : "Unknown error occurred";
    return NextResponse.json({ error: message }, { status: 500 });
  }
}

async function generateChangelogFromManifest(
  repoPath: string,
  manifest: AocManifest
): Promise<string> {
  // Collect analysis artifacts for all trace_ids
  const commitSummaries: string[] = [];
  for (const traceId of manifest.trace_ids) {
    const artifact = await getAnalysisArtifact(repoPath, traceId);
    if (artifact) {
      commitSummaries.push(`- ${artifact.bump}: v${artifact.version}`);
    }
  }

  const userPrompt = `Generate a markdown changelog entry for a release from v${manifest.initial_version} to v${manifest.final_version}.

Session label: ${manifest.label}
${manifest.commit_count} commits across ${manifest.cluster_count} change clusters.
${manifest.test_failures} test failures recorded.

Commit summaries:
${commitSummaries.join("\n")}

Write a "## What's Changed" section with bullet points grouped by bump type. Keep it concise.`;

  const systemPrompt = `You are a release notes author. Summarize software commits into a clean markdown changelog entry.`;

  const response = await inferenceChat([
    { role: "system", content: systemPrompt },
    { role: "user", content: userPrompt },
  ]);

  return response.trim();
}
