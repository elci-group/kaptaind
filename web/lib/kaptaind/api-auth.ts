import { NextResponse } from "next/server";
import { getServerSession } from "next-auth";
import { authorizeProjectRequestCore } from "./api-auth-core";
import { authOptions } from "../auth";
import { resolveRepoPath } from "./reader";

type AuthDeps = Partial<Parameters<typeof authorizeProjectRequestCore>[1]>;

export type ProjectAuthorization =
  | {
      ok: true;
      projectId: string;
      repoPath: string;
      userId: string;
    }
  | {
      ok: false;
      response: NextResponse;
    };

export async function authorizeProjectRequest(
  req: Request,
  deps: AuthDeps = {}
): Promise<ProjectAuthorization> {
  const result = await authorizeProjectRequestCore(req, {
    getSession: deps?.getSession ?? (() => getServerSession(authOptions)),
    resolvePath: deps?.resolvePath ?? resolveRepoPath,
  });
  if (result.ok) {
    return result;
  }

  return {
    ok: false,
    response: NextResponse.json(result.body, { status: result.status }),
  };
}
