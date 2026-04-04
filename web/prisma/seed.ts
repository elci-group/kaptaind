import { PrismaClient } from "@prisma/client";
import bcrypt from "bcryptjs";

const prisma = new PrismaClient();

async function main() {
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

  // Create subscription for test user
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

  console.log(`Seeded database with test user and project`);
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
