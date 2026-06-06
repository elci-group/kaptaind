import { NextResponse } from "next/server";
import {
  requireAuth,
  requireProjectAccess,
  isAuthError,
} from "@/lib/api-auth";
import { resolveRepoPath } from "@/lib/kaptaind/reader";
import {
  listAnalysisArtifacts,
  getAnalysisArtifact,
} from "@/lib/kaptaind/analysis";

export async function GET(req: Request) {
  try {
    await requireAuth(req);

    const { searchParams } = new URL(req.url);
    const projectId = searchParams.get("projectId");
    const clusterId = searchParams.get("clusterId");
    const limit = parseInt(searchParams.get("limit") || "50", 10);

    if (!projectId) {
      return NextResponse.json(
        { error: "projectId required" },
        { status: 400 }
      );
    }

    await requireProjectAccess(req, projectId);

    const repoPath = await resolveRepoPath(projectId);

    if (clusterId) {
      const artifact = await getAnalysisArtifact(repoPath, clusterId);
      return NextResponse.json(artifact);
    }

    const artifacts = await listAnalysisArtifacts(repoPath, limit);
    return NextResponse.json(artifacts);
  } catch (error) {
    const authError = isAuthError(error);
    if (authError) {
      return NextResponse.json(
        { error: authError.message },
        { status: authError.status }
      );
    }
    return NextResponse.json(
      { error: "Failed to read analysis artifacts" },
      { status: 500 }
    );
  }
}
