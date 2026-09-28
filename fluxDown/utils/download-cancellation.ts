/**
 * Start cancelling a browser download before resolving its filename pipeline.
 *
 * Chrome's filename pipeline waits for suggest. Request cancellation before
 * releasing that pipeline, and avoid holding suggest until cancellation settles:
 * a pending filename decision could otherwise block cancellation progress.
 */
export async function cancelBeforeFilenameResolution(
  downloadId: number,
  cancel: (downloadId: number) => Promise<void>,
  erase: (query: { id: number }) => Promise<unknown>,
  resolveFilename: () => void,
): Promise<boolean> {
  let cancellation: Promise<void>;
  try {
    cancellation = cancel(downloadId);
  } catch {
    resolveFilename();
    return false;
  }

  resolveFilename();

  try {
    await cancellation;
  } catch {
    return false;
  }

  try {
    await erase({ id: downloadId });
  } catch {
    // The cancellation succeeded; erasing only removes browser history residue.
  }
  return true;
}
