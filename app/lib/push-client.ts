// Browser push is outside the mobile notification scope.
export function startPushNotifications(): () => void { return () => undefined; }
export async function unregisterPushDevice(): Promise<void> {}
