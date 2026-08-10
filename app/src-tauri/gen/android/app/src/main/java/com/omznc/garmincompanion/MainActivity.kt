package com.omznc.garmincompanion

import android.app.Activity
import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.ClipData
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.content.pm.PackageInstaller
import android.graphics.Color
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.provider.Settings
import android.view.View
import android.webkit.JavascriptInterface
import android.webkit.WebView
import androidx.activity.OnBackPressedCallback
import androidx.activity.enableEdgeToEdge
import androidx.core.content.ContextCompat
import androidx.core.content.FileProvider
import androidx.core.content.IntentCompat
import androidx.core.graphics.ColorUtils
import androidx.core.view.ViewCompat
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import org.json.JSONObject
import java.io.File

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
  }

  /**
   * Called by `WryActivity.setWebView`, which runs at construction and before
   * the first `loadUrl` — so anything bound here is on `window` by the time the
   * app's first line runs, with no round trip.
   *
   * That timing is the whole reason this is a JavaScript interface rather than
   * a Tauri command. The palette decides what colour the page is; asking for it
   * asynchronously would mean painting one frame in the wrong one on every
   * launch, which is the same thing `lib/theme.ts` goes to the trouble of
   * mirroring the selected theme in `localStorage` to avoid.
   *
   * The webview only ever loads this app's own bundled assets, so there is no
   * third-party script here for the binding to be reachable from.
   */
  override fun onWebViewCreate(webView: WebView) {
    webView.addJavascriptInterface(SystemPalette(this, webView), "__GARMIN_ANDROID__")
    // Bound here for company rather than for timing — unlike the palette,
    // nothing about an update has to be answerable before the first paint.
    webView.addJavascriptInterface(ApkInstaller(this, webView), "__GARMIN_INSTALL__")
    // Same reasoning as the installer above: bound here for company, not for
    // timing. Nothing about sharing has to be answerable before first paint.
    webView.addJavascriptInterface(ShareSheet(this), "__GARMIN_SHARE__")
    handleBack(webView)
  }

  /**
   * Back, behaving the way it does in every other app on the phone.
   *
   * `TauriActivity` turns `WryActivity`'s own back handling off and puts
   * nothing in its place, so the press reached the default and closed the app —
   * from a lap list, from Settings, from four screens deep. Back is the one
   * control every Android user reaches for without looking, and it did the most
   * destructive thing available, everywhere.
   *
   * Two questions, asked in the order they matter. What is *open* is the app's
   * to answer — a bottom sheet is not a history entry, and back has to close it
   * rather than navigate out from under it, so `lib/back.ts` gets the press
   * first. Where you have *been* the webview already knows exactly, hash routes
   * included, so `canGoBack()` answers that without the page having to shadow
   * it with a counter of its own.
   *
   * `evaluateJavascript` lands its result a frame or so later, which is below
   * the threshold where a back press feels delayed and is the price of letting
   * the page decide at all.
   */
  private fun handleBack(webView: WebView) {
    onBackPressedDispatcher.addCallback(this, object : OnBackPressedCallback(true) {
      override fun handleOnBackPressed() {
        // Missing means the bundle hasn't run yet — nothing is open, because
        // nothing has been drawn.
        webView.evaluateJavascript(
          "window.__GARMIN_BACK__ ? window.__GARMIN_BACK__() : false",
        ) { handled ->
          if (handled == "true") return@evaluateJavascript
          if (webView.canGoBack()) {
            webView.goBack()
          } else {
            // Nowhere left to go, so let the press through to the default —
            // which leaves the app the way the system expects, animation
            // included, rather than calling `finish()` over the top of it.
            isEnabled = false
            onBackPressedDispatcher.onBackPressed()
            isEnabled = true
          }
        }
      }
    })
  }
}

/**
 * The wallpaper's colours, and who gets to say what the status bar looks like.
 *
 * Read by `lib/dynamic.ts`. Both halves are here rather than in two places
 * because they are the same fact seen twice — Material You is only coherent if
 * the app's surface and the system bars above and below it agree.
 */
class SystemPalette(private val activity: Activity, private val webView: WebView) {
  /**
   * Bumped by every decision about the inset, so that a watcher left over from
   * an earlier one stops rather than undoing the current one behind its back.
   */
  private var generation = 0

  /**
   * The five system tonal ramps, as `{"accent1_500": "#7b5ea7", …}`.
   *
   * All 65 rather than the dozen the app maps onto its own palette: the mapping
   * is a design decision and belongs on the TypeScript side next to the rest of
   * the colour logic, and the contrast guard there needs neighbouring tones to
   * step to. Reading the extra fifty is a handful of resource lookups once per
   * launch.
   *
   * Empty on anything before Android 12, which has no dynamic colour to read —
   * the app falls back to its own palette, same as every desktop build.
   */
  @JavascriptInterface
  fun dynamicColors(): String {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.S) return ""

    val out = JSONObject()
    for (ramp in RAMPS) {
      for (tone in TONES) {
        val name = "system_${ramp}_$tone"
        // By name rather than by `android.R.color.system_…` constant: the
        // alternative is sixty-five `when` branches to turn a string into an
        // int the framework will look up by name anyway. These are framework
        // resources, so nothing in this app's build can shrink them away.
        val id = activity.resources.getIdentifier(name, "color", "android")
        if (id == 0) continue
        out.put("${ramp}_$tone", String.format("#%06x", activity.getColor(id) and 0xFFFFFF))
      }
    }
    return out.toString()
  }

  /**
   * Which way round the status and navigation bar icons are drawn.
   *
   * `enableEdgeToEdge()` sets this from the system's light/dark setting, which
   * is the wrong question: the app has its own appearance and the two disagree
   * whenever someone picks light while the phone is dark, or wears a palette
   * with a fixed appearance of its own. The result is white icons on white.
   *
   * `light` is the app's own resolved mode, pushed down from `paint()` in
   * `lib/theme.ts` — so this follows the palette rather than the phone.
   */
  @JavascriptInterface
  fun setBarAppearance(light: Boolean) {
    activity.runOnUiThread {
      val window = activity.window
      // "Light bars" means dark icons, for a light background behind them.
      WindowCompat.getInsetsController(window, window.decorView).apply {
        isAppearanceLightStatusBars = light
        isAppearanceLightNavigationBars = light
      }
    }
  }

  /**
   * Whether the window is allowed to draw under the system bars and the cutout.
   *
   * On for everything this app renders: the shell pads itself out of the way
   * with `env(safe-area-inset-*)`, the scroll fade softens whatever passes
   * behind the status bar, and the result is a page that runs to the edge of
   * the screen the way a native one does.
   *
   * Off for the one page in the window that isn't this app. Sign-in loads
   * Garmin's own HTML into this same webview (there is only one on a phone —
   * see `login.rs`), and their markup lays out at y=0 because it has never
   * heard of this window. Edge-to-edge puts their header under the notification
   * bar and behind the camera. Nothing here can reach into their page, so the
   * window stops being edge-to-edge for as long as their page is in it.
   *
   * Padding the content view rather than `setDecorFitsSystemWindows`: the
   * insets are consumed at the one view the webview hangs off, which leaves
   * `enableEdgeToEdge()`'s transparent bars — and every other window flag —
   * exactly as they were, so turning this back on is just removing the padding.
   * `statusBarColor` would have been the obvious lever and is a no-op from
   * API 35, which is what the backdrop below replaces: the freed band is the
   * content view's own padding, so its background is what shows there.
   *
   * `displayCutout` as well as `systemBars` because on a phone held upright the
   * camera is *inside* the status bar and the two insets coincide — but in
   * landscape the cutout moves to a side the status bar doesn't cover, and
   * `systemBars` alone would let the page slide under it.
   *
   * Turning it back on is watched for here rather than waited for from the page
   * — see [watchForReturn].
   */
  @JavascriptInterface
  fun setEdgeToEdge(on: Boolean, backdrop: String) {
    activity.runOnUiThread {
      // Any explicit call settles the question, so a watcher still running from
      // an earlier one has nothing left to decide.
      generation += 1
      apply(on, backdrop)
      if (on) return@runOnUiThread

      val home = Uri.parse(webView.url ?: return@runOnUiThread).host ?: return@runOnUiThread
      watchForReturn(generation, home)
    }
  }

  /** The inset itself. UI thread; [setEdgeToEdge] is the guarded way in. */
  private fun apply(on: Boolean, backdrop: String) {
    val content = activity.findViewById<View>(android.R.id.content) ?: return

    if (on) {
      ViewCompat.setOnApplyWindowInsetsListener(content, null)
      content.setPadding(0, 0, 0, 0)
      content.setBackgroundColor(Color.TRANSPARENT)
    } else {
      val color = runCatching { Color.parseColor(backdrop) }.getOrDefault(Color.BLACK)
      content.setBackgroundColor(color)
      // The bars sit on the backdrop now, not on the page, so their icons
      // follow it — a white sign-in page needs dark icons whatever the app's
      // own palette happens to be.
      setBarAppearance(ColorUtils.calculateLuminance(color) > 0.5)
      ViewCompat.setOnApplyWindowInsetsListener(content) { view, insets ->
        val bars = insets.getInsets(
          WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout(),
        )
        view.setPadding(bars.left, bars.top, bars.right, bars.bottom)
        insets
      }
    }
    ViewCompat.requestApplyInsets(content)
  }

  /**
   * Put the window back edge-to-edge once the webview is showing this app again.
   *
   * The page is the obvious place to ask for this and turned out to be the wrong
   * one: the only page that could is the one that loads *after* sign-in, and
   * whether it gets the chance depends on a navigation that this app doesn't
   * drive, doesn't observe, and doesn't always survive. Missing it leaves the
   * window inset with a white band across the top for the rest of the session,
   * with a force-quit as the only way out — a worse failure than the one the
   * inset exists to prevent.
   *
   * So the window watches instead of being told. `home` is the host the webview
   * was on when it was asked to step aside, which is this app's own — whatever
   * it happens to be on this platform and this build. Coming back to it is the
   * whole condition, so nothing here needs to know that Garmin is where it went.
   *
   * [left] is the reason this isn't a one-line check: the request comes in
   * *before* the navigation it is making room for, so the first ticks still see
   * home and would undo the inset immediately. Restoring is only right after
   * having actually gone somewhere.
   *
   * The give-up is for the navigation that never happens — sign-in failing
   * before it starts. Nothing came to be inset for, so the inset shouldn't
   * outlive the attempt.
   *
   * Polling because a `WebViewClient` is wry's and a page hook is generated
   * code. Reading a URL every half second, only while the window is stood
   * aside, is a smaller thing than either.
   */
  private fun watchForReturn(gen: Int, home: String) {
    var waited = 0L
    var left = false

    webView.postDelayed(object : Runnable {
      override fun run() {
        if (gen != generation) return

        val host = Uri.parse(webView.url ?: "").host
        if (host != home) {
          left = true
        } else if (left) {
          generation += 1
          apply(true, "")
          return
        }

        waited += POLL_MS
        if (!left && waited >= GIVE_UP_MS) {
          generation += 1
          apply(true, "")
          return
        }
        webView.postDelayed(this, POLL_MS)
      }
    }, POLL_MS)
  }

  private companion object {
    val RAMPS = listOf("accent1", "accent2", "accent3", "neutral1", "neutral2")

    /** Android's tone ladder, lightest first — 0 is white, 1000 is black. */
    val TONES = listOf(0, 10, 50, 100, 200, 300, 400, 500, 600, 700, 800, 900, 1000)

    /** Fast enough that the band is gone before the returning page has painted. */
    const val POLL_MS = 500L

    /** How long a navigation that never came gets before the inset is dropped. */
    const val GIVE_UP_MS = 15_000L
  }
}

/**
 * Handing a downloaded APK to the system so it can replace this app with it.
 *
 * Read by `lib/android.ts`; the version check and the download itself are the
 * TypeScript and Rust sides of `lib/updater.ts`. Only the last step needs to be
 * here, because only the last step is something Android has an opinion about.
 *
 * **What this is not.** It is not a silent install — that is reserved for
 * device owners, and no amount of permission gets a sideloaded app there. The
 * system draws its own confirmation over the app, and the user taps Update.
 * Two taps total, from a phone that would otherwise never learn a new version
 * exists.
 *
 * **Why it's safe to offer at all.** Not the permission — the signature.
 * Android will only install *over* an existing package when the new one is
 * signed with the same key, so the worst a wrong APK can do here is get offered
 * as an unrelated second app, under its own name, in a dialog that says so.
 * That check is the system's, runs after this class is done, and can't be
 * waived. See RELEASING for what that key is and why losing it is terminal.
 */
class ApkInstaller(private val activity: Activity, private val webView: WebView) {
  /** The action the session reports back on. Ours alone — see [listen]. */
  private val action = "${activity.packageName}.INSTALL_STATUS"
  private var listening = false

  /**
   * Whether the phone would let us install anything at all.
   *
   * "Install unknown apps" is granted per-app from Android 8, and the app that
   * has it is whichever *browser* the APK was downloaded through — never this
   * one, on a first update. So the honest first answer here is no, and
   * [openPermissionSettings] is the way out of it. Below 8 the setting is
   * global and was already turned on to sideload this in the first place.
   */
  @JavascriptInterface
  fun canInstall(): Boolean =
    Build.VERSION.SDK_INT < Build.VERSION_CODES.O ||
      activity.packageManager.canRequestPackageInstalls()

  /**
   * The system page that grants it, scoped to this app.
   *
   * There is no dialog for this permission and no callback when it's granted —
   * it's a settings screen with a toggle, and the only way back is the user
   * returning on their own. So nothing is awaited: the UI goes back to offering
   * the install, and [canInstall] answers differently next time it's asked.
   */
  @JavascriptInterface
  fun openPermissionSettings() {
    activity.startActivity(
      Intent(
        Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES,
        Uri.parse("package:${activity.packageName}"),
      ),
    )
  }

  /**
   * Stage the APK at [path] and ask the system to install it.
   *
   * Returns "" when the request went in, or a sentence when it didn't. Note the
   * asymmetry: "" means the *dialog* is coming, not that the install worked.
   * Whether it does is decided minutes later, by a user tapping a button this
   * app doesn't own, and comes back through [listen] instead.
   *
   * `PackageInstaller` rather than an `ACTION_VIEW` at the file: the intent
   * route needs a content URI, a `FileProvider` grant and a MIME type Android
   * has deprecated twice, and it hands over a file the system has to trust the
   * path of. A session is bytes streamed from this process into one the
   * installer already owns, which is both the supported API and the one that
   * doesn't care where the file lives.
   */
  @JavascriptInterface
  fun install(path: String): String {
    val apk = File(path)
    if (!apk.isFile) return "the downloaded file is no longer there"
    if (!canInstall()) return "this app isn't allowed to install apps yet"

    return try {
      listen()
      val installer = activity.packageManager.packageInstaller
      val params = PackageInstaller.SessionParams(
        PackageInstaller.SessionParams.MODE_FULL_INSTALL,
      )
      // Named, so the system can tell before opening the file that this is an
      // update to us and not a new app — which is what lets its dialog say
      // "update" rather than "install", and what makes a mismatched signature
      // fail here rather than after the user has agreed to something.
      params.setAppPackageName(activity.packageName)

      val id = installer.createSession(params)
      installer.openSession(id).use { session ->
        session.openWrite(NAME, 0, apk.length()).use { out ->
          apk.inputStream().use { it.copyTo(out) }
          // Without this the bytes may still be in a buffer when commit()
          // reads the session, and the install fails on a truncated APK.
          session.fsync(out)
        }
        session.commit(pending(id).intentSender)
      }
      ""
    } catch (e: Exception) {
      e.message ?: e.javaClass.simpleName
    }
  }

  /**
   * The session's way of reaching back into the app.
   *
   * Two things arrive here and they are not the same kind of thing.
   * `STATUS_PENDING_USER_ACTION` is the system saying "ask them" and handing
   * over the dialog to launch — committing a session does not put anything on
   * screen by itself, and skipping this leaves an install that silently never
   * happens. Everything else is the outcome, which is worth a sentence in the
   * UI because the likeliest one is `INSTALL_FAILED_UPDATE_INCOMPATIBLE`: a
   * debug build being handed a release APK, signed with a different key. The
   * system's own toast for that says "App not installed" and no more.
   *
   * Registered on the application context, once, and never unregistered — on
   * the activity it would outlive one and leak. Not exported: nothing outside
   * this app has any business sending it.
   */
  private fun listen() {
    if (listening) return
    listening = true

    ContextCompat.registerReceiver(
      activity.applicationContext,
      object : BroadcastReceiver() {
        override fun onReceive(context: Context, intent: Intent) {
          val status = intent.getIntExtra(PackageInstaller.EXTRA_STATUS, -1)
          if (status == PackageInstaller.STATUS_PENDING_USER_ACTION) {
            val confirm =
              IntentCompat.getParcelableExtra(intent, Intent.EXTRA_INTENT, Intent::class.java)
            // NEW_TASK because this is the application context's, not the
            // activity's — there is no task of ours for it to land in.
            confirm?.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            confirm?.let(activity::startActivity)
            return
          }
          // Success is the one outcome nothing needs to be told about: the
          // process this would report to is about to be replaced.
          if (status == PackageInstaller.STATUS_SUCCESS) return

          val why = intent.getStringExtra(PackageInstaller.EXTRA_STATUS_MESSAGE)
            ?: "the install was cancelled"
          report(why)
        }
      },
      IntentFilter(action),
      ContextCompat.RECEIVER_NOT_EXPORTED,
    )
  }

  /** Mutable because the system fills the result in; per-session so two don't collide. */
  private fun pending(id: Int): PendingIntent = PendingIntent.getBroadcast(
    activity,
    id,
    Intent(action).setPackage(activity.packageName),
    PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_MUTABLE,
  )

  /** Back to `lib/updater.ts`, which owns what the Settings screen says. */
  private fun report(message: String) {
    val quoted = JSONObject.quote(message)
    activity.runOnUiThread {
      webView.evaluateJavascript(
        "window.__GARMIN_INSTALL_FAILED__ && window.__GARMIN_INSTALL_FAILED__($quoted)",
        null,
      )
    }
  }

  private companion object {
    /** The name of the APK *inside* the session; the installer picks its own path. */
    const val NAME = "update.apk"
  }
}

/**
 * The system sharesheet, over a card the frontend rendered.
 *
 * The whole class is the file handover, because that's the only hard part.
 * `share.rs` writes the PNG into this app's private cache, which is a path no
 * other process can open — handing a raw `file://` to another app has thrown
 * `FileUriExposedException` since Android 7. `FileProvider` turns it into a
 * `content://` URI backed by this app, and the read flag grants exactly the app
 * the user picks, for exactly as long as the share lasts.
 *
 * Nothing comes back. Android does not tell the sender which target was chosen,
 * or whether the sheet was dismissed without choosing one, so the button says
 * the sheet went up and stops there rather than claiming a share it can't see.
 */
class ShareSheet(private val activity: Activity) {
  @JavascriptInterface
  fun share(path: String): String {
    val card = File(path)
    if (!card.isFile) return "the card is no longer there"

    return try {
      val uri = FileProvider.getUriForFile(
        activity,
        "${activity.packageName}.fileprovider",
        card,
      )
      val send = Intent(Intent.ACTION_SEND).apply {
        type = MIME
        putExtra(Intent.EXTRA_STREAM, uri)
        addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        // The extra alone is enough for apps that read it, and there are
        // enough that don't — the preview in the sheet itself is drawn from
        // the clip data, so without this the user picks a target for an image
        // they can't see.
        clipData = ClipData.newUri(activity.contentResolver, "Card", uri)
      }
      activity.startActivity(Intent.createChooser(send, null))
      ""
    } catch (e: Exception) {
      e.message ?: e.javaClass.simpleName
    }
  }

  private companion object {
    const val MIME = "image/png"
  }
}
