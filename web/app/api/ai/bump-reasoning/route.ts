import { NextResponse } from "next/server";
import { getServerSession } from "next-auth";
import { authOptions } from "@/lib/auth";
import { resolveRepoPath } from "@/lib/kaptaind/reader";
import { getAnalysisArtifact } from "@/lib/kaptaind/analysis";
import { inferenceChat } from "@/lib/inference";
import type { AnalysisArtifact } from "@/types/kaptaind";

export async function POST(req: Request) {
  const session = await getServerSession(authOptions);
  if (!session?.user) {
    return NextResponse.json({ error: "Unauthorized" }, { status: 401 });
  }

  try {
    const body = await req.json();
    const { projectId, clusterId } = body;

    if (!projectId || !clusterId) {
      return NextResponse.json(
        { error: "projectId and clusterId required" },
        { status: 400 }
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
