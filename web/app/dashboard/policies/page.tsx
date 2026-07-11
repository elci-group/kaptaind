import { getServerSession } from "next-auth";
import { redirect } from "next/navigation";
import { authOptions } from "@/lib/auth";
import { getEntitlements } from "@/lib/entitlements";
import { requirePolicyPacks } from "@/lib/policy";
import { prisma } from "@/lib/prisma";
import Card, { CardHeader, CardTitle } from "@/components/ui/Card";
import ProGate from "@/components/dashboard/ProGate";
import PolicyEditor from "@/components/dashboard/PolicyEditor";

const PROJECT_ID = "default";

export default async function PoliciesPage() {
  const session = await getServerSession(authOptions);
  if (!session?.user?.id) redirect("/auth/signin");

  const userId = session.user.id;
  const entitlements = await getEntitlements({ userId });
  if (!entitlements.canUsePolicyPacks)
    return <ProGate feature="Policy Packs" />;

  const project = await prisma.project.findFirst({
    where: {
      id: PROJECT_ID,
      OR: [{ ownerId: userId }, { memberships: { some: { userId } } }],
    },
    select: { id: true, orgId: true },
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

  try {
    await requirePolicyPacks({ userId, orgId: project.orgId || undefined });
  } catch {
    return <ProGate feature="Policy Packs" />;
  }

  const policy = await prisma.policy.findUnique({
    where: { projectId: PROJECT_ID },
  });

  const parsedPolicy = policy
    ? {
        versionBumpRules: policy.versionBumpRules
          ? JSON.parse(policy.versionBumpRules)
          : null,
        branchProtections: policy.branchProtections
          ? JSON.parse(policy.branchProtections)
          : null,
        minimumTests: policy.minimumTests
          ? JSON.parse(policy.minimumTests)
          : null,
        disallowedFilePatterns: policy.disallowedFilePatterns
          ? JSON.parse(policy.disallowedFilePatterns)
          : null,
        releaseQualificationThresholds: policy.releaseQualificationThresholds
          ? JSON.parse(policy.releaseQualificationThresholds)
          : null,
      }
    : null;

  return (
    <div className="p-8">
      <div className="mb-8">
        <h1 className="text-2xl font-bold text-zinc-900 dark:text-zinc-100">
          Policies
        </h1>
        <p className="text-sm text-zinc-500 dark:text-zinc-400">
          Manage versioning and release policies for the default project.
        </p>
      </div>

      <Card>
        <CardHeader>
          <CardTitle>Project Policy</CardTitle>
        </CardHeader>
        <PolicyEditor projectId={PROJECT_ID} initialPolicy={parsedPolicy} />
      </Card>
    </div>
  );
}
