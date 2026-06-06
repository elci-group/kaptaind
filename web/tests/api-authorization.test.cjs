/* eslint-disable @typescript-eslint/no-require-imports */

const assert = require("node:assert/strict");
const test = require("node:test");

process.env.TS_NODE_COMPILER_OPTIONS = JSON.stringify({
  module: "CommonJS",
  moduleResolution: "node",
});
require("ts-node/register/transpile-only");

const { projectAccessWhere } = require("../lib/kaptaind/access.ts");
const { authorizeProjectRequestCore } = require("../lib/kaptaind/api-auth-core.ts");
const { handleTracesRequestCore } = require("../lib/kaptaind/traces-route-core.ts");

test("project access filter allows project owner", () => {
  assert.deepEqual(projectAccessWhere("project-1", "user-1"), {
    id: "project-1",
    OR: [
      { ownerId: "user-1" },
      { memberships: { some: { userId: "user-1" } } },
    ],
  });
});

test("project access filter scopes membership to the active user", () => {
  const where = projectAccessWhere("project-2", "member-2");

  assert.equal(where.id, "project-2");
  assert.deepEqual(where.OR[1], {
    memberships: { some: { userId: "member-2" } },
  });
});

test("project request authorization rejects unauthenticated users", async () => {
  const result = await authorizeProjectRequestCore(
    new Request("http://localhost/api/kaptaind/status?projectId=project-1"),
    {
      getSession: async () => null,
    }
  );

  assert.equal(result.ok, false);
  assert.equal(result.status, 401);
  assert.deepEqual(result.body, { error: "Unauthorized" });
});

test("project request authorization requires projectId", async () => {
  const result = await authorizeProjectRequestCore(
    new Request("http://localhost/api/kaptaind/status"),
    {
      getSession: async () => ({ user: { id: "user-1" } }),
    }
  );

  assert.equal(result.ok, false);
  assert.equal(result.status, 400);
  assert.deepEqual(result.body, { error: "projectId required" });
});

test("project request authorization rejects inaccessible projects", async () => {
  const result = await authorizeProjectRequestCore(
    new Request("http://localhost/api/kaptaind/status?projectId=project-3"),
    {
      getSession: async () => ({ user: { id: "user-1" } }),
      resolvePath: async () => {
        throw new Error("not found");
      },
    }
  );

  assert.equal(result.ok, false);
  assert.equal(result.status, 403);
  assert.deepEqual(result.body, { error: "Forbidden" });
});

test("project request authorization returns authorized repo path", async () => {
  const result = await authorizeProjectRequestCore(
    new Request("http://localhost/api/kaptaind/status?projectId=project-4"),
    {
      getSession: async () => ({ user: { id: "user-4" } }),
      resolvePath: async (projectId, userId) => {
        assert.equal(projectId, "project-4");
        assert.equal(userId, "user-4");
        return "/tmp/project-4";
      },
    }
  );

  assert.equal(result.ok, true);
  assert.equal(result.projectId, "project-4");
  assert.equal(result.userId, "user-4");
  assert.equal(result.repoPath, "/tmp/project-4");
});

test("traces route core authorizes before requiring aocId", async () => {
  const result = await handleTracesRequestCore(
    new Request("http://localhost/api/kaptaind/traces?projectId=project-5"),
    {
      getSession: async () => null,
      resolvePath: async () => {
        throw new Error("should not resolve");
      },
      readTraces: async () => {
        throw new Error("should not read");
      },
    }
  );

  assert.equal(result.status, 401);
  assert.deepEqual(result.body, { error: "Unauthorized" });
});

test("traces route core requires aocId after authorization", async () => {
  const result = await handleTracesRequestCore(
    new Request("http://localhost/api/kaptaind/traces?projectId=project-6"),
    {
      getSession: async () => ({ user: { id: "user-6" } }),
      resolvePath: async () => "/tmp/project-6",
      readTraces: async () => {
        throw new Error("should not read");
      },
    }
  );

  assert.equal(result.status, 400);
  assert.deepEqual(result.body, { error: "aocId required" });
});

test("traces route core returns traces for authorized requests", async () => {
  const result = await handleTracesRequestCore(
    new Request("http://localhost/api/kaptaind/traces?projectId=project-7&aocId=aoc-1&limit=7"),
    {
      getSession: async () => ({ user: { id: "user-7" } }),
      resolvePath: async () => "/tmp/project-7",
      readTraces: async (repoPath, aocId, limit) => {
        assert.equal(repoPath, "/tmp/project-7");
        assert.equal(aocId, "aoc-1");
        assert.equal(limit, 7);
        return [{ id: "trace-1" }];
      },
    }
  );

  assert.equal(result.status, 200);
  assert.deepEqual(result.body, [{ id: "trace-1" }]);
});
