import { api } from "./api";
import { toast } from "sonner";

/**
 * Download the offered update and, where the platform can, install it.
 *
 * One function because there are two Update buttons — the shell banner and the Account page — and
 * the interesting half of this is platform behaviour, not presentation. Two copies would drift, and
 * the copy that drifted would be the one nobody tested on a phone.
 *
 * The shape of the problem: on desktop, "downloaded to ~/Downloads" is a complete answer, because
 * the package manager is the right installer and that path is somewhere the user can reach. On
 * Android neither is true — the file lands in app-specific storage that cannot be browsed to, and
 * nothing opens it. So the same button has to mean "download" in one place and "download, then hand
 * to the system installer" in the other.
 *
 * Returns true when something actually happened, false when the user still has a step to take.
 */
export async function downloadAndInstallUpdate() {
  const r = await api.downloadUpdate();

  let state = null;
  try {
    state = await api.updateInstallState();
  } catch {
    // An older backend, or a platform without the command. Fall through to the desktop wording,
    // which is never wrong — only sometimes unhelpful.
  }

  if (!state?.supported) {
    toast.success(`Downloaded to ${r.path}`, { duration: 12000 });
    if (r.next) toast.message(r.next, { duration: 14000 });
    return true;
  }

  if (!state.allowed) {
    toast.message(
      "Android needs your permission to install updates. Turn on \"Allow from this source\" on the "
      + "screen that opens, then press Update again.",
      { duration: 12000 },
    );
    try {
      await api.requestInstallPermission();
    } catch {
      // The screen could not be opened; the toast above already says what is needed, and the
      // Permissions step in Setup offers the same button.
    }
    return false;
  }

  await api.installDownloadedUpdate(r.path);
  toast.success("Handed to the installer — Android will ask you to confirm.", { duration: 10000 });
  return true;
}
