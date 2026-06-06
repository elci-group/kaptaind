import { authorizeProjectRequestCore } from "./api-auth-core";

type AuthDeps = Parameters<typeof authorizeProjectRequestCore>[1];

type TracesRouteDeps = AuthDeps & {
  readTraces: (repoPath: string, aocId: string, limit: number) => Promise<unknown>;
};

export type RouteCoreResult = {
  status: number;
  body: unknown;
};

export async function handleTracesRequestCore(
  req: Request,
  deps: TracesRouteDeps
): Promise<RouteCoreResult> {
  const auth = await authorizeProjectRequestCore(req, deps);
  if (!auth.ok) {
    return { status: auth.status, body: auth.body };
  }

  const { searchParams } = new URL(req.url);
  const aocId = searchParams.get("aocId");
  if (!aocId) {
    return { status: 400, body: { error: "aocId required" } };
  }

  const limit = parseInt(searchParams.get("limit") || "100", 10);

  try {
    return {
      status: 200,
      body: await deps.readTraces(auth.repoPath, aocId, limit),
    };
  } catch {
    return {
      status: 500,
      body: { error: "Failed to read traces" },
    };
  }
}
