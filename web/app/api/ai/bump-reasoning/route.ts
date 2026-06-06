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

    const reasoning = await generateBumpReasoning(artifact);
    return NextResponse.json({ reasoning });
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

async function generateBumpReasoning(artifact: AnalysisArtifact): Promise<string> {
  const userPrompt = `Explain in 2–4 sentences why this change received a ${artifact.bump} version bump (not another level).

Composite score: ${artifact.weight.score.toFixed(3)}
Scores: structural=${artifact.diff.structural.toFixed(3)} api=${artifact.diff.api.toFixed(3)} deps=${artifact.diff.deps.toFixed(3)} runtime=${artifact.diff.runtime.toFixed(3)}
API breaking: ${artifact.diff.api_breaking} / API added: ${artifact.diff.api_added}
API touches: ${artifact.diff.api_touches}, signatures changed: ${artifact.diff.api_signatures}

Be concrete and brief, referencing the metrics.`;

  const systemPrompt = `You are a semantic versioning advisor. Explain concisely why a change received a specific version bump level.`;

  const response = await inferenceChat([
    { role: "system", content: systemPrompt },
    { role: "user", content: userPrompt },
  ]);

  return response.trim();
}
