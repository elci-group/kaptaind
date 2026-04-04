import { NextResponse } from "next/server";
import { getServerSession } from "next-auth";
import { authOptions } from "@/lib/auth";
import { resolveRepoPath } from "@/lib/kaptaind/reader";
import { readTracesForAoc } from "@/lib/kaptaind/traces";

export async function GET(req: Request) {
  const session = await getServerSession(authOptions);
  if (!session?.user) {
    return NextResponse.json({ error: "Unauthorized" }, { status: 401 });
  }

  const { searchParams } = new URL(req.url);
  const projectId = searchParams.get("projectId");
  const aocId = searchParams.get("aocId");
  const limit = parseInt(searchParams.get("limit") || "100", 10);

  if (!projectId || !aocId) {
    return NextResponse.json(
      { error: "projectId and aocId required" },
      { status: 400 }
    );
  }

  try {
    const repoPath = await resolveRepoPath(projectId);
    const traces = await readTracesForAoc(repoPath, aocId, limit);
    return NextResponse.json(traces);
  } catch (error) {
    return NextResponse.json(
      { error: "Failed to read traces" },
      { status: 500 }
    );
  }
}
