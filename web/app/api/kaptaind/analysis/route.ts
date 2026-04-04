import { NextResponse } from "next/server";
import { getServerSession } from "next-auth";
import { authOptions } from "@/lib/auth";
import { resolveRepoPath } from "@/lib/kaptaind/reader";
import {
  listAnalysisArtifacts,
  getAnalysisArtifact,
} from "@/lib/kaptaind/analysis";

export async function GET(req: Request) {
  const session = await getServerSession(authOptions);
  if (!session?.user) {
    return NextResponse.json({ error: "Unauthorized" }, { status: 401 });
  }

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

  try {
    const repoPath = await resolveRepoPath(projectId);

    if (clusterId) {
      const artifact = await getAnalysisArtifact(repoPath, clusterId);
      return NextResponse.json(artifact);
    }

    const artifacts = await listAnalysisArtifacts(repoPath, limit);
    return NextResponse.json(artifacts);
  } catch (error) {
    return NextResponse.json(
      { error: "Failed to read analysis artifacts" },
      { status: 500 }
    );
  }
}
