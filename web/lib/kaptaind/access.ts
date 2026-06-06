export function projectAccessWhere(projectId: string, userId: string) {
  return {
    id: projectId,
    OR: [
      { ownerId: userId },
      { memberships: { some: { userId } } },
    ],
  };
}
