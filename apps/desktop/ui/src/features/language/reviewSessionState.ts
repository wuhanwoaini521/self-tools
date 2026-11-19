/** Remove the card just submitted without advancing past the next item. */
export function afterReviewSubmission<T>(
  queue: readonly T[],
  index: number,
): { queue: T[]; index: number } {
  const nextQueue = queue.filter((_, position) => position !== index);
  return {
    queue: nextQueue,
    // Keep the next item in place, or move back when the submitted card was last.
    index: Math.min(index, Math.max(0, nextQueue.length - 1)),
  };
}
