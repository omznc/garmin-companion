package expo.modules.apkupdater

import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.content.pm.PackageInstaller
import android.net.Uri
import android.os.Build
import android.provider.Settings
import androidx.core.content.ContextCompat
import androidx.core.content.IntentCompat
import expo.modules.kotlin.exception.Exceptions
import expo.modules.kotlin.functions.Coroutine
import expo.modules.kotlin.modules.Module
import expo.modules.kotlin.modules.ModuleDefinition
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.io.File
import java.net.HttpURLConnection
import java.net.URL

/**
 * Self-update of the standalone APK. Port of the old Tauri `ApkInstaller`.
 *
 * Android refuses to install over this app an APK that has a different
 * signing key. The hash check only stops a broken download from reaching the
 * installer.
 */
class ApkUpdaterModule : Module() {
  private val context: Context
    get() = appContext.reactContext ?: throw Exceptions.ReactContextLost()

  private val action: String
    get() = "${context.packageName}.INSTALL_STATUS"

  private val updatesDir: File
    get() = File(context.cacheDir, "updates")

  private var receiver: BroadcastReceiver? = null

  override fun definition() = ModuleDefinition {
    Name("ApkUpdater")
    Events("progress", "installFailed")

    Function("installedVersion") { installedVersion() }
    Function("isNewer") { version: String -> Apk.isNewer(version, installedVersion()) }
    Function("canInstall") { canInstall() }
    Function("openPermissionSettings") { openPermissionSettings() }

    AsyncFunction("download") Coroutine { url: String, version: String, sha256: String ->
      withContext(Dispatchers.IO) { download(url, version, sha256.lowercase()) }
    }
    AsyncFunction("install") Coroutine { path: String ->
      withContext(Dispatchers.IO) { install(File(path)) }
    }

    OnDestroy { receiver?.let { context.unregisterReceiver(it) } }
  }

  private fun installedVersion(): String =
    context.packageManager.getPackageInfo(context.packageName, 0).versionName ?: ""

  private fun canInstall(): Boolean =
    Build.VERSION.SDK_INT < Build.VERSION_CODES.O ||
      context.packageManager.canRequestPackageInstalls()

  private fun openPermissionSettings() {
    val intent = Intent(
      Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES,
      Uri.parse("package:${context.packageName}"),
    ).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
    context.startActivity(intent)
  }

  /** Returns the path of a verified APK. Throws if the version or the hash is wrong. */
  private fun download(url: String, version: String, want: String): String {
    if (!Apk.isNewer(version, installedVersion())) error("version $version is not newer")
    updatesDir.mkdirs()
    val apk = File(updatesDir, "garmin-companion_$version.apk")
    // Remove older downloads. They are tens of megabytes each.
    updatesDir.listFiles()?.filter { it != apk }?.forEach { it.delete() }
    if (apk.isFile && apk.inputStream().use(Apk::sha256) == want) return apk.path

    val part = File(updatesDir, "${apk.name}.part")
    val conn = URL(url).openConnection() as HttpURLConnection
    try {
      if (conn.responseCode !in 200..299) error("the download is not there (${conn.responseCode})")
      copyWithProgress(conn, part)
    } finally {
      conn.disconnect()
    }
    // The name never points to a partial or a wrong file.
    if (part.inputStream().use(Apk::sha256) != want) {
      part.delete()
      error("the download did not arrive intact")
    }
    if (!part.renameTo(apk)) error("could not finish the download")
    return apk.path
  }

  private fun copyWithProgress(conn: HttpURLConnection, part: File) {
    val total = conn.contentLengthLong.coerceAtLeast(0)
    var received = 0L
    var announced = 0L
    conn.inputStream.use { input ->
      part.outputStream().use { out ->
        val buffer = ByteArray(64 * 1024)
        while (true) {
          val n = input.read(buffer)
          if (n < 0) break
          out.write(buffer, 0, n)
          received += n
          if (received - announced >= 256 * 1024) {
            announced = received
            sendEvent("progress", mapOf("received" to received, "total" to total))
          }
        }
      }
    }
    sendEvent("progress", mapOf("received" to received, "total" to maxOf(total, received)))
  }

  /** Stage the APK in a PackageInstaller session. The result comes back as an event. */
  private fun install(apk: File) {
    if (apk.parentFile?.canonicalPath != updatesDir.canonicalPath || !apk.isFile) {
      error("the file is not a downloaded update")
    }
    if (!canInstall()) error("this app is not allowed to install apps yet")
    val archive = context.packageManager.getPackageArchiveInfo(apk.path, 0)
    if (archive?.packageName != context.packageName ||
      !Apk.isNewer(archive.versionName ?: "", installedVersion())
    ) {
      error("the file is not a newer version of this app")
    }

    listen()
    val installer = context.packageManager.packageInstaller
    val params = PackageInstaller.SessionParams(PackageInstaller.SessionParams.MODE_FULL_INSTALL)
    params.setAppPackageName(context.packageName)
    val id = installer.createSession(params)
    installer.openSession(id).use { session ->
      session.openWrite("update.apk", 0, apk.length()).use { out ->
        apk.inputStream().use { it.copyTo(out) }
        session.fsync(out)
      }
      session.commit(pending(id).intentSender)
    }
  }

  private fun pending(id: Int): PendingIntent = PendingIntent.getBroadcast(
    context,
    id,
    Intent(action).setPackage(context.packageName),
    // Mutable, because the system fills in the result.
    PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_MUTABLE,
  )

  private fun listen() {
    if (receiver != null) return
    val r = object : BroadcastReceiver() {
      override fun onReceive(c: Context, intent: Intent) = onStatus(intent)
    }
    receiver = r
    ContextCompat.registerReceiver(
      context, r, IntentFilter(action), ContextCompat.RECEIVER_NOT_EXPORTED,
    )
  }

  private fun onStatus(intent: Intent) {
    val status = intent.getIntExtra(PackageInstaller.EXTRA_STATUS, -1)
    when (status) {
      // The system asks us to show its dialog.
      PackageInstaller.STATUS_PENDING_USER_ACTION -> {
        val confirm = IntentCompat.getParcelableExtra(intent, Intent.EXTRA_INTENT, Intent::class.java)
        confirm?.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        confirm?.let(context::startActivity)
      }
      // The new app replaces this process. Nothing to report.
      PackageInstaller.STATUS_SUCCESS -> Unit
      else -> {
        val why = intent.getStringExtra(PackageInstaller.EXTRA_STATUS_MESSAGE)
          ?: "the install was cancelled"
        sendEvent("installFailed", mapOf("message" to why))
      }
    }
  }
}
