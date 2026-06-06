import { NextResponse } from "next/server";
import {
  requireAuth,
  requireProjectAccess,
  requireEntitlement,
  getUserTier,
  isAuthError,
} from "@/lib/api-auth";
import { rateLimit } from "@/lib/rate-limit";
import { resolveRepoPath } from "@/lib/kaptaind/reader";
import { getAnalysisArtifact } from "@/lib/kaptaind/analysis";
import { listAocManifests } from "@/lib/kaptaind/aoc";
import { inferenceChat } from "@/lib/inference";
import type { AocManifest } from "@/types/kaptaind";

export async function POST(req: Request) {
  try {
    const session = await requireAuth(req);

    const body = await req.json();
    const { projectId, aocId } = body;

    if (!projectId || !aocId) {
      return NextResponse.json(
        { error: "projectId and aocId required" },
        { status: 400 }
      );
    }

    await requireProjectAccess(req, projectId);
    await requireEntitlement(req, "canUseAi");

    const tier = await getUserTier(session.user.id);
    const limitResult = await rateLimit(req, {
      type: "ai",
      userId: session.user.id,
      tier,
    });
    if (!limitResult.allowed) {
      return NextResponse.json(
        { error: "Rate limit exceeded" },
        {
          status: 429,
          headers: { "Retry-After": String(limitResult.retryAfter) },
        }
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
    const authError = isAuthError(error);
    if (authError) {
      return NextResponse.json(
        { error: authError.message },
        { status: authError.status }
      );
    }
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
