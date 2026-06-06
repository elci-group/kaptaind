type SessionUser = {
  id?: string;
};

type Session = {
  user?: SessionUser | null;
} | null;

type AuthDeps = {
  getSession: () => Promise<Session>;
  resolvePath: (projectId: string, userId: string) => Promise<string>;
};

export type ProjectAuthorizationCore =
  | {
      ok: true;
      projectId: string;
      repoPath: string;
      userId: string;
    }
  | {
      ok: false;
      status: number;
      body: { error: string };
    };

export async function authorizeProjectRequestCore(
  req: Request,
  deps: AuthDeps
): Promise<ProjectAuthorizationCore> {
  const session = await deps.getSession();
  const userId = session?.user?.id;
  if (!userId) {
    return {
      ok: false,
      status: 401,
      body: { error: "Unauthorized" },
    };
  }

  const { searchParams } = new URL(req.url);
  const projectId = searchParams.get("projectId");
  if (!projectId) {
    return {
      ok: false,
      status: 400,
      body: { error: "projectId required" },
    };
  }

  try {
    return {
      ok: true,
      projectId,
      repoPath: await deps.resolvePath(projectId, userId),
      userId,
    };
  } catch {
    return {
      ok: false,
      status: 403,
      body: { error: "Forbidden" },
    };
  }
}
