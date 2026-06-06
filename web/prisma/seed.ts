import { PrismaClient } from "@prisma/client";
import bcrypt from "bcryptjs";

const prisma = new PrismaClient();

async function main() {
  // Seed Plans
  const plans = [
    {
      code: "free",
      name: "Free / OSS",
      entitlements: [
        { featureKey: "canUseAi", featureValue: "false" },
        { featureKey: "maxRepos", featureValue: "1" },
        { featureKey: "maxUsers", featureValue: "1" },
        { featureKey: "retentionDays", featureValue: "0" },
        { featureKey: "canUseSso", featureValue: "false" },
        { featureKey: "canUsePolicyPacks", featureValue: "false" },
        { featureKey: "canExportAuditLogs", featureValue: "false" },
      ],
    },
    {
      code: "pro",
      name: "Pro",
      entitlements: [
        { featureKey: "canUseAi", featureValue: "true" },
        { featureKey: "maxRepos", featureValue: "10" },
        { featureKey: "maxUsers", featureValue: "1" },
        { featureKey: "retentionDays", featureValue: "30" },
        { featureKey: "canUseSso", featureValue: "false" },
        { featureKey: "canUsePolicyPacks", featureValue: "false" },
        { featureKey: "canExportAuditLogs", featureValue: "false" },
      ],
    },
    {
      code: "team",
      name: "Team",
      entitlements: [
        { featureKey: "canUseAi", featureValue: "true" },
        { featureKey: "maxRepos", featureValue: "50" },
        { featureKey: "maxUsers", featureValue: "25" },
        { featureKey: "retentionDays", featureValue: "180" },
        { featureKey: "canUseSso", featureValue: "false" },
        { featureKey: "canUsePolicyPacks", featureValue: "true" },
        { featureKey: "canExportAuditLogs", featureValue: "false" },
      ],
    },
    {
      code: "enterprise",
      name: "Enterprise",
      entitlements: [
        { featureKey: "canUseAi", featureValue: "true" },
        { featureKey: "maxRepos", featureValue: "100000" },
        { featureKey: "maxUsers", featureValue: "100000" },
        { featureKey: "retentionDays", featureValue: "3650" },
        { featureKey: "canUseSso", featureValue: "true" },
        { featureKey: "canUsePolicyPacks", featureValue: "true" },
        { featureKey: "canExportAuditLogs", featureValue: "true" },
      ],
    },
  ];

  for (const plan of plans) {
    const upserted = await prisma.plan.upsert({
      where: { code: plan.code },
      update: { name: plan.name },
      create: { code: plan.code, name: plan.name },
    });

    for (const ent of plan.entitlements) {
      await prisma.entitlement.upsert({
        where: {
          planId_featureKey: {
            planId: upserted.id,
            featureKey: ent.featureKey,
          },
        },
        update: { featureValue: ent.featureValue },
        create: {
          planId: upserted.id,
          featureKey: ent.featureKey,
          featureValue: ent.featureValue,
        },
      });
    }
  }

  console.log("Seeded plans and entitlements");

  if (process.env.NODE_ENV === "development") {
    // Create test user
    const testUser = await prisma.user.upsert({
      where: { email: "test@example.com" },
      update: {},
      create: {
        email: "test@example.com",
        name: "Test User",
        passwordHash: await bcrypt.hash("password123", 10),
      },
    });

    // Create legacy subscription for test user
    await prisma.subscription.upsert({
      where: { userId: testUser.id },
      update: {},
      create: {
        userId: testUser.id,
        stripeCustomerId: "cus_test_123",
        tier: "pro",
        status: "active",
        currentPeriodEnd: new Date(Date.now() + 30 * 24 * 60 * 60 * 1000),
      },
    });

    // Create default project pointing at kaptaind repo
    const repoPath = process.env.KAPTAIND_REPO_PATH || "/home/adminx/kaptaind";
    const project = await prisma.project.upsert({
      where: { id: "default" },
      update: {},
      create: {
        id: "default",
        ownerId: testUser.id,
        name: "Kaptaind",
        repoPath: repoPath,
        description: "Automated semantic versioning daemon",
      },
    });

    // Add test user to project
    await prisma.teamMembership.upsert({
      where: {
        projectId_userId: { projectId: project.id, userId: testUser.id },
      },
      update: {},
      create: {
        projectId: project.id,
        userId: testUser.id,
        role: "owner",
      },
    });

    console.log("Created test user, subscription, and project");
  } else {
    console.log("Skipping test user seed (not in development)");
  }
}

main()
  .then(async () => {
    await prisma.$disconnect();
  })
  .catch(async (e) => {
    console.error(e);
    await prisma.$disconnect();
    process.exit(1);
  });
