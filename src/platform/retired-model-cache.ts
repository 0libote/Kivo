/** Remove only the retired browser speech integration's caches, never native models. */
export async function clearRetiredModelCaches(storage: Pick<CacheStorage, "keys" | "delete">) {
  const names = await storage.keys();
  const retired = names.filter(
    (name) =>
      name.startsWith("desert-ant-voz-") ||
      name.startsWith("kivo-voz-state") ||
      name === "desert-ant-models",
  );
  await Promise.all(retired.map((name) => storage.delete(name)));
}
