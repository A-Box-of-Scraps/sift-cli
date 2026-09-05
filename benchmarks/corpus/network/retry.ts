export function retryWithBackoff(attempt: number): number {
  // Retry failed requests with exponential delay.
  return Math.min(30000, 100 * 2 ** attempt);
}
