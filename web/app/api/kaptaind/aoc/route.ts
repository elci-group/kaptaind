import { NextResponse } from "next/server";
import { getServerSession } from "next-auth";
import { authOptions } from "@/lib/auth";
import { resolveRepoPath } from "@/lib/kaptaind/reader";
import { readActiveSession, listAocManifests } from "@/lib/kaptaind/aoc";

export async function GET(req: Request) {
  const session = await getServerSession(authOptions);
  if (!session?.user) {
    return NextResponse.json({ error: "Unauthorized" }, { status: 401 });
  }

  const { searchParams } = new URL(req.url);
  const projectId = searchParams.get("projectId");
  const active = searchParams.get("active") === "true";
  const limit = parseInt(searchParams.get("limit") || "50", 10);

  if (!projectId) {
    return NextResponse.json(
      { error: "projectId required" },
      { status: 400 }
    );
  }

  try {
    const repoPath = await resolveRepoPath(projectId);

    if (active) {
      const session = await readActiveSession(repoPath);
      return NextResponse.json(session);
    }

    const manifests = await listAocManifests(repoPath, limit);
    return NextResponse.json(manifests);
  } catch (error) {
    return NextResponse.json(
      { error: "Failed to read AoC data" },
      { status: 500 }
    );
  }
}
