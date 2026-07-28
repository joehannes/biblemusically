package studio.lightkid.app

import android.app.Activity
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.provider.Settings
import androidx.core.content.FileProvider
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import java.io.File

/**
 * Handing a downloaded update to Android's package installer.
 *
 * The desktop story for updating is "download the installer and let the OS take it from there"; the
 * app deliberately does not replace its own binary. Android has the same shape but none of the same
 * plumbing, and without this class the Update button on a phone downloaded a file and stopped:
 * nothing could open it, and it landed in app-specific storage that a user cannot easily browse to.
 *
 * Three separate things have to be true before an install can even be offered, and they fail in
 * different ways, so each is a separate call rather than one hopeful attempt:
 *
 *   1. The manifest declares REQUEST_INSTALL_PACKAGES. That is a build-time fact, not a runtime one.
 *   2. The *user* has granted this app "install unknown apps". Android 8 made that per-app instead of
 *      one global switch, which is a real improvement — but it means it can be missing even though
 *      the permission is declared, and it cannot be requested with the normal runtime-permission
 *      dialog. The only route is to send the user to a Settings screen. See [requestInstallPermission].
 *   3. The file is reachable by another process. It is not, by default: passing a file:// URI across
 *      an app boundary throws FileUriExposedException since Android 7, so it goes through the
 *      FileProvider declared in the manifest with a read grant attached to the intent.
 *
 * None of this installs anything silently, and it is not capable of doing so. The system installer
 * still shows its own confirmation, and Android still refuses an APK signed with a different key than
 * the copy already installed — which is the check that actually protects the user here.
 */
@InvokeArg
class InstallArgs {
  lateinit var path: String
}

@TauriPlugin
class InstallPlugin(private val activity: Activity) : Plugin(activity) {

  /**
   * Whether an install could be offered right now, and if not, which of the two reasons applies.
   *
   * Reported rather than acted on: the caller decides whether this is the moment to send somebody to
   * a Settings screen. Below Android 8 there is no per-app grant to check, so the answer is yes.
   */
  @Command
  fun canInstall(invoke: Invoke) {
    val res = JSObject()
    val supported = Build.VERSION.SDK_INT >= Build.VERSION_CODES.O
    val allowed = if (supported) activity.packageManager.canRequestPackageInstalls() else true
    res.put("allowed", allowed)
    res.put("needsSettings", supported && !allowed)
    invoke.resolve(res)
  }

  /**
   * Open the one Settings screen that can grant "install unknown apps" for this app.
   *
   * Resolves as soon as the screen is opened, not when a decision is made — Android gives no result
   * back for this, so the honest thing is to re-ask [canInstall] afterwards rather than to pretend a
   * return value means consent.
   */
  @Command
  fun requestInstallPermission(invoke: Invoke) {
    try {
      if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O &&
          !activity.packageManager.canRequestPackageInstalls()) {
        val intent = Intent(
          Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES,
          Uri.parse("package:${activity.packageName}"),
        ).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        activity.startActivity(intent)
        invoke.resolve(JSObject().apply { put("opened", true) })
      } else {
        invoke.resolve(JSObject().apply { put("opened", false) })
      }
    } catch (ex: Exception) {
      invoke.reject(ex.message ?: "Could not open the install-permission screen.")
    }
  }

  /** Hand an APK already on disk to the system installer. */
  @Command
  fun installApk(invoke: Invoke) {
    try {
      val args = invoke.parseArgs(InstallArgs::class.java)
      val file = File(args.path)
      if (!file.exists()) {
        invoke.reject("The downloaded update is no longer on disk: ${args.path}")
        return
      }
      if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O &&
          !activity.packageManager.canRequestPackageInstalls()) {
        // Better a clear refusal than an intent the system silently drops.
        invoke.reject("This app has not been allowed to install updates yet.")
        return
      }

      val uri: Uri = FileProvider.getUriForFile(
        activity,
        "${activity.packageName}.fileprovider",
        file,
      )
      val intent = Intent(Intent.ACTION_VIEW).apply {
        setDataAndType(uri, "application/vnd.android.package-archive")
        addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
      }
      activity.startActivity(intent)
      invoke.resolve(JSObject().apply { put("handedOff", true) })
    } catch (ex: Exception) {
      invoke.reject(ex.message ?: "The update could not be handed to the installer.")
    }
  }
}
