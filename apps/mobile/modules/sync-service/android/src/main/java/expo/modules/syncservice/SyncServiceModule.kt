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

    Function("stop") { context.stopService(Intent(context, SyncService::class.java)) }
  }

  /** False if the system does not allow a foreground service now. The exception of API 31 for
   *  this case (ForegroundServiceStartNotAllowedException) is an IllegalStateException. */
  private fun start(context: Context): Boolean =
    try {
      ContextCompat.startForegroundService(context, Intent(context, SyncService::class.java))
      true
    } catch (e: IllegalStateException) {
      false
    }
}
