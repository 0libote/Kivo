/** Serializes model use and deletion, and invalidates work requested before removal. */
export class OperationQueue {
  private tail: Promise<unknown> = Promise.resolve();
  private generation = 0;

  invalidate() {
    this.generation += 1;
  }

  run<T>(operation: (assertCurrent: () => void) => Promise<T>): Promise<T> {
    const generation = this.generation;
    const assertCurrent = () => {
      if (generation !== this.generation) throw new Error("The model operation was cancelled.");
    };
    const result = this.tail.then(async () => {
      assertCurrent();
      return operation(assertCurrent);
    });
    // A failed operation must not poison the queue or produce an unhandled rejection.
    this.tail = result.catch(() => undefined);
    return result;
  }
}

/** A cancellation during an async marker write must roll that marker back. */
export async function publishReadiness(
  assertCurrent: () => void,
  setReady: (ready: boolean) => Promise<void>,
) {
  assertCurrent();
  try {
    await setReady(true);
    assertCurrent();
  } catch (cause) {
    await setReady(false);
    throw cause;
  }
}
