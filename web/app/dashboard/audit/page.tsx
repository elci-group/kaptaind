import { getServerSession } from "next-auth";
import { redirect } from "next/navigation";
import { authOptions } from "@/lib/auth";
import { getEntitlements } from "@/lib/entitlements";
import { prisma } from "@/lib/prisma";
import Card, { CardTitle } from "@/components/ui/Card";
import { Table, Thead, Th, Td } from "@/components/ui/Table";
import Badge from "@/components/ui/Badge";
import ProGate from "@/components/dashboard/ProGate";

const PROJECT_ID = "default";

export default async function AuditPage() {
  const session = await getServerSession(authOptions);
  if (!session?.user?.id) redirect("/auth/signin");

  const userId = session.user.id;
  const entitlements = await getEntitlements({ userId });
  if (!entitlements.canExportAuditLogs)
    return <ProGate feature="Audit Logs" />;

  // Verify access to default project
  const project = await prisma.project.findFirst({
    where: {
      id: PROJECT_ID,
      OR: [{ ownerId: userId }, { memberships: { some: { userId } } }],
    },
    select: { id: true },
  });

  if (!project) {
    return (
      <div className="flex flex-1 items-center justify-center p-8">
        <Card className="max-w-md text-center">
          <h2 className="mb-2 text-xl font-semibold text-zinc-900 dark:text-zinc-100">
            No Project Access
          </h2>
          <p className="text-sm text-zinc-500 dark:text-zinc-400">
            You do not have access to the default project.
          </p>
        </Card>
      </div>
    );
  }

  // Fetch audit logs server-side via internal helper for simplicity,
  // or we could call the API route. Using prisma directly avoids
  // needing to construct an absolute URL for fetch in SSR.
  const logs = await prisma.auditLog.findMany({
    where: { projectId: PROJECT_ID },
    orderBy: { timestamp: "desc" },
    take: 100,
  });

  return (
    <div className="p-8">
      <div className="mb-8">
        <h1 className="text-2xl font-bold text-zinc-900 dark:text-zinc-100">
          Audit Log
        </h1>
        <p className="text-sm text-zinc-500 dark:text-zinc-400">
          Recent activity for the default project.
        </p>
      </div>

      <Card className="p-0">
        <div className="p-6 pb-0">
          <CardTitle>Events</CardTitle>
        </div>
        <div className="mt-4">
          {logs.length === 0 ? (
            <p className="px-6 pb-6 text-sm text-zinc-500 dark:text-zinc-400">
              No audit events found.
            </p>
          ) : (
            <Table>
              <Thead>
                <tr>
                  <Th>Timestamp</Th>
                  <Th>Actor</Th>
                  <Th>Action</Th>
                  <Th>Resource</Th>
                  <Th>Before Hash</Th>
                  <Th>After Hash</Th>
                </tr>
              </Thead>
              <tbody className="divide-y divide-zinc-200 dark:divide-zinc-800">
                {logs.map((log) => (
                  <tr key={log.id}>
                    <Td>
                      {new Date(log.timestamp).toLocaleString()}
                    </Td>
                    <Td>{log.actor}</Td>
                    <Td>
                      <Badge variant="info">{log.action}</Badge>
                    </Td>
                    <Td>{log.resource}</Td>
                    <Td>
                      <span className="font-mono text-xs text-zinc-500 dark:text-zinc-400">
                        {log.beforeHash || "—"}
                      </span>
                    </Td>
                    <Td>
                      <span className="font-mono text-xs text-zinc-500 dark:text-zinc-400">
                        {log.afterHash || "—"}
                      </span>
                    </Td>
                  </tr>
                ))}
              </tbody>
            </Table>
          )}
        </div>
      </Card>
    </div>
  );
}
