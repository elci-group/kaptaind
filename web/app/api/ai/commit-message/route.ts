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
import { inferenceChat } from "@/lib/inference";
import type { AnalysisArtifact } from "@/types/kaptaind";

export async function POST(req: Request) {
  try {
    const session = await requireAuth(req);

    const body = await req.json();
    const { projectId, clusterId } = body;

    if (!projectId || !clusterId) {
      return NextResponse.json(
        { error: "projectId and clusterId required" },
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
    const artifact = await getAnalysisArtifact(repoPath, clusterId);

    if (!artifact) {
      return NextResponse.json(
        { error: "Analysis artifact not found" },
        { status: 404 }
      );
    }

    const narrative = await generateCommitNarrative(artifact);
    return NextResponse.json({ narrative });
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

async function generateCommitNarrative(artifact: AnalysisArtifact): Promise<string> {
  const apiSummary = artifact.diff.api_breaking
    ? "api-breaking"
    : artifact.diff.api_added
      ? "api-added"
      : "api-stable";

  const userPrompt = `Describe this code change in one subject line (max 72 chars).

Bump type: ${artifact.bump} (Patch=tweak, Minor=new feature, Major=breaking)
Version: v${artifact.version}
API status: ${apiSummary}
Composite score: ${artifact.weight.score.toFixed(3)}

Scores: structural=${artifact.diff.structural.toFixed(3)} api=${artifact.diff.api.toFixed(3)} deps=${artifact.diff.deps.toFixed(3)} runtime=${artifact.diff.runtime.toFixed(3)}
API touches: ${artifact.diff.api_touches}, signatures changed: ${artifact.diff.api_signatures}
Dependency nodes affected: ${artifact.diff.dependency_nodes}`;

  const systemPrompt = `You are a precise software commit message author. Write a single subject line (max 72 characters) describing what changed. Use conventional commit format (feat:, fix:, refactor:, chore:) when it fits. Output ONLY the subject line — no body, no explanation.`;

  const response = await inferenceChat([
    { role: "system", content: systemPrompt },
    { role: "user", content: userPrompt },
  ]);

  return response.trim().split("\n")[0];
}
