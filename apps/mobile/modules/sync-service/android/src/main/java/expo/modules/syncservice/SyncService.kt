package expo.modules.syncservice

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import android.os.PowerManager
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat

/**
 * A foreground service that runs while the history import runs. Android 15 blocks the network
 * of a background app. A foreground service of type dataSync keeps the network open, and the
 * wake lock keeps the CPU awake. The Rust sync runs in the app process, so the service has no work
 * of its own.
 */
class SyncService : Service() {
  private var wakeLock: PowerManager.WakeLock? = null
  private var inForeground = false

  override fun onBind(intent: Intent?): IBinder? = null

  // The foreground state starts in onCreate, before any start or stop command, so a stop that
  // comes at once can not stop the service before startForeground.
  override fun onCreate() {
    super.onCreate()
    try {
      ServiceCompat.startForeground(
        this,
        NOTIFICATION_ID,
        notification(),
        ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC,
      )
      inForeground = true
    } catch (e: RuntimeException) {
      // The system refused the foreground state. Without it the service has no use.
      stopSelf()
    }
  }

  override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
    if (intent?.action == ACTION_STOP) {
      releaseWakeLock()
      // With the start id, a start that came after this stop keeps the service.
      stopSelf(startId)
    } else if (inForeground) {
      acquireWakeLock()
    }
    // The import is not resumable from the service. The app resumes it at the next start.
    return START_NOT_STICKY
  }

  // The system calls this when the dataSync time budget ends. Without a stop the system crashes
  // the app.
  override fun onTimeout(startId: Int, fgsType: Int) {
    releaseWakeLock()
    stopSelf()
  }

  override fun onTaskRemoved(rootIntent: Intent?) {
    releaseWakeLock()
    stopSelf()
  }

  override fun onDestroy() {
    releaseWakeLock()
    super.onDestroy()
  }

  private fun acquireWakeLock() {
    if (wakeLock?.isHeld == true) return
    val power = getSystemService(Context.POWER_SERVICE) as PowerManager
    wakeLock =
      power.newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "garmincompanion:sync").apply {
        acquire(WAKE_LOCK_TIMEOUT_MS)
      }
  }

  private fun releaseWakeLock() {
    wakeLock?.let { if (it.isHeld) it.release() }
    wakeLock = null
  }

  private fun notification(): Notification {
    val manager = getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
      manager.createNotificationChannel(
        NotificationChannel(CHANNEL_ID, "Sync", NotificationManager.IMPORTANCE_LOW),
      )
    }
    // expo-notifications makes the drawable "notification_icon" from the plugin icon.
    val named = resources.getIdentifier("notification_icon", "drawable", packageName)
    val tap =
      packageManager.getLaunchIntentForPackage(packageName)?.let {
        PendingIntent.getActivity(this, 0, it, PendingIntent.FLAG_IMMUTABLE)
      }
    return NotificationCompat.Builder(this, CHANNEL_ID)
      .setSmallIcon(if (named != 0) named else applicationInfo.icon)
      .setContentTitle(applicationInfo.loadLabel(packageManager))
      .setContentText("Importing your history")
      .setOngoing(true)
      .setOnlyAlertOnce(true)
      .setContentIntent(tap)
      .setForegroundServiceBehavior(NotificationCompat.FOREGROUND_SERVICE_IMMEDIATE)
      .build()
  }

  companion object {
    const val ACTION_STOP = "expo.modules.syncservice.STOP"
    private const val CHANNEL_ID = "sync"
    private const val NOTIFICATION_ID = 4101
    private const val WAKE_LOCK_TIMEOUT_MS = 6 * 60 * 60 * 1000L
  }
}
