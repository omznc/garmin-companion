package expo.modules.nightmode

import android.content.Context
import androidx.appcompat.app.AppCompatDelegate

/**
 * The light or dark mode that the person chose in Appearance. JavaScript
 * keeps the choice in its own store, but that store is read only after the
 * activity starts. A native dialog takes its theme from the night mode at the
 * start of the activity, so a later change left the dialogs light. This copy
 * of the choice is read before the activity starts.
 *
 * `MainActivity` calls `apply` two times (the plugin
 * `plugins/night-mode.js` adds the calls):
 * - In `attachBaseContext`, before AppCompat makes the configuration of the
 *   activity.
 * - After `super.onCreate`, because `expo-system-ui` sets the mode of the
 *   app config ("automatic") there.
 */
object NightMode {
  private const val PREFERENCES = "gc_night_mode"
  private const val KEY = "scheme"

  /** Keeps the choice: "system", "light" or "dark". */
  fun save(context: Context, scheme: String) {
    context.getSharedPreferences(PREFERENCES, Context.MODE_PRIVATE)
      .edit()
      .putString(KEY, scheme)
      .apply()
  }

  /** Sets the kept choice as the night mode of the app. */
  @JvmStatic
  fun apply(context: Context) {
    val scheme = context.getSharedPreferences(PREFERENCES, Context.MODE_PRIVATE)
      .getString(KEY, null)
    AppCompatDelegate.setDefaultNightMode(modeOf(scheme))
  }

  private fun modeOf(scheme: String?): Int = when (scheme) {
    "dark" -> AppCompatDelegate.MODE_NIGHT_YES
    "light" -> AppCompatDelegate.MODE_NIGHT_NO
    else -> AppCompatDelegate.MODE_NIGHT_FOLLOW_SYSTEM
  }
}
