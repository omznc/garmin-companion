package expo.modules.syncservice

import android.content.Context
import android.content.Intent
import androidx.core.content.ContextCompat
import expo.modules.kotlin.exception.Exceptions
import expo.modules.kotlin.modules.Module
import expo.modules.kotlin.modules.ModuleDefinition

/** Gives JavaScript the `start` and `stop` of the sync service. */
class SyncServiceModule : Module() {
  private val context: Context
    get() = appContext.reactContext?.applicationContext ?: throw Exceptions.ReactContextLost()

  override fun definition() = ModuleDefinition {
    Name("SyncService")

    Function("start") { start(context) }

    Function("stop") { stop(context) }
  }

  /** False if the system does not allow a foreground service now, for example in the background
   *  (ForegroundServiceStartNotAllowedException) or without the type permission (SecurityException). */
  private fun start(context: Context): Boolean =
    try {
      ContextCompat.startForegroundService(context, Intent(context, SyncService::class.java))
      true
    } catch (e: RuntimeException) {
      false
    }

  /** The stop goes in the same queue as the start, so it comes after onCreate and startForeground.
   *  If the service is not in the foreground, the app is in the background and the system refuses
   *  the command. Then stop the service directly. */
  private fun stop(context: Context) {
    val intent = Intent(context, SyncService::class.java).setAction(SyncService.ACTION_STOP)
    try {
      context.startService(intent)
    } catch (e: RuntimeException) {
      context.stopService(Intent(context, SyncService::class.java))
    }
  }
}
