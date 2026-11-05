export function accessMessage(status: number | undefined): string | null {
  if (status === 401) return 'Authentication required (401). Sign-in is not yet available in this app.';
  if (status === 403) return 'Access denied (403). Your account does not have permission to access this resource.';
  return null;
}

export function createWorkspaceAccess() {
  let generation = 0;
  let denied = false;
  return {
    snapshot: () => generation,
    current: (snapshot: number) => !denied && snapshot === generation,
    retry: () => { denied = false; return ++generation; },
    deny: (status: number | undefined) => {
      const message = accessMessage(status);
      if (message) { denied = true; generation += 1; }
      return message;
    },
  };
}
