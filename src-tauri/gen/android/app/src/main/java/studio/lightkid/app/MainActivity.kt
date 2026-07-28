package studio.lightkid.app

import android.Manifest
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import androidx.activity.enableEdgeToEdge
import androidx.core.app.ActivityCompat
import androidx.core.content.ContextCompat

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
    ensureAudioPermission()
  }

  /**
   * Ask Android for the microphone.
   *
   * The manifest declaration alone is not enough on Android 6 and later: RECORD_AUDIO is a runtime
   * permission, and until the *app* holds it there is nothing a WebView could grant a page even if
   * it wanted to. That is what made the guide's "Allow microphone" button report a refusal nobody
   * had made — no system dialog ever appeared, so `getUserMedia` failed with what looks from
   * JavaScript exactly like a user pressing Deny.
   *
   * Asked at startup rather than at the moment of use. The alternative puts Android's dialog on top
   * of the guide screen that explains why it is being asked for, which is the wrong order for the
   * person reading it.
   */
  private fun ensureAudioPermission() {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.M) return
    val granted = ContextCompat.checkSelfPermission(this, Manifest.permission.RECORD_AUDIO) ==
      PackageManager.PERMISSION_GRANTED
    if (!granted) {
      ActivityCompat.requestPermissions(this, arrayOf(Manifest.permission.RECORD_AUDIO), REQ_AUDIO)
    }
  }

  companion object {
    private const val REQ_AUDIO = 4101
  }
}
