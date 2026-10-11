import { describe, expect, it } from "bun:test";
import { OperationQueue, publishReadiness } from "./operation-queue";

describe("model operation lifecycle", () => {
  it("removal invalidates an active load and queued inference before either publishes", async () => {
    const queue = new OperationQueue();
    const started = Promise.withResolvers<void>();
    const release = Promise.withResolvers<void>();
    let ready = false;
    const load = queue.run(async (assertCurrent) => {
      started.resolve();
      await release.promise;
      assertCurrent();
      ready = true;
    });
    const loadRejected = load.catch((cause: unknown) => cause);
    await started.promise;
    const inference = queue.run(async () => "transcript");
    const inferenceRejected = inference.catch((cause: unknown) => cause);
    queue.invalidate();
    const removal = queue.run(async () => {
      ready = false;
    });
    release.resolve();
    const [loadError, inferenceError] = await Promise.all([
      loadRejected,
      inferenceRejected,
      removal,
    ]);
    expect(String(loadError)).toContain("cancelled");
    expect(String(inferenceError)).toContain("cancelled");
    expect(ready).toBe(false);
    expect(await queue.run(async () => "new installation")).toBe("new installation");
  });

  it("never overlaps model users, even after failure", async () => {
    const queue = new OperationQueue();
    const events: string[] = [];
    const first = queue.run(async () => {
      events.push("first");
      throw new Error("download failed");
    });
    const rejected = expect(first).rejects.toThrow("download failed");
    const second = queue.run(async () => {
      events.push("second");
      return 42;
    });
    await rejected;
    expect(await second).toBe(42);
    expect(events).toEqual(["first", "second"]);
  });
});

it("rolls back readiness when removal arrives during the marker write", async () => {
  const queue = new OperationQueue();
  const started = Promise.withResolvers<void>();
  const release = Promise.withResolvers<void>();
  let ready = false;
  const result = queue.run((assertCurrent) =>
    publishReadiness(assertCurrent, async (value) => {
      ready = value;
      if (value) {
        started.resolve();
        await release.promise;
      }
    }),
  );
  const rejected = result.catch((cause: unknown) => cause);
  await started.promise;
  queue.invalidate();
  release.resolve();
  expect(String(await rejected)).toContain("cancelled");
  expect(ready).toBe(false);
});
