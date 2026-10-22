// Config plugin: the window asks for the screen's top refresh rate.
// Left alone, Android 15 and later run a window at the rate of its views'
// frame-rate category. On a 120 Hz phone that maps "normal" to 60 Hz and "high"
// (a finger down) to 90, so the app never reaches 120. A window that names a mode
// gets it. Both fields, because a screen with one mode and several rates takes the
// mode and still lowers the rate unless asked for that too.
const { withMainActivity } = require("expo/config-plugins");

const MARKER = "askForTopRefreshRate";
const ON_CREATE = "    super.onCreate(null)\n";
const CLASS_LINE = "class MainActivity : ReactActivity() {\n";

const METHOD = `  private fun ${MARKER}() {
    @Suppress("DEPRECATION")
    val display = windowManager.defaultDisplay
    val now = display.mode
    val top = display.supportedModes
      .filter { it.physicalWidth == now.physicalWidth && it.physicalHeight == now.physicalHeight }
      .maxByOrNull { it.refreshRate } ?: return
    if (top.refreshRate <= 61f) return
    window.attributes = window.attributes.apply {
      preferredDisplayModeId = top.modeId
      preferredRefreshRate = top.refreshRate
    }
  }

`;

module.exports = function withTopRefreshRate(config) {
  return withMainActivity(config, (activity) => {
    if (activity.modResults.language !== "kt") {
      throw new Error("withTopRefreshRate: MainActivity must be Kotlin.");
    }
    let source = activity.modResults.contents;
    if (source.includes(MARKER)) return activity;
    if (!source.includes(ON_CREATE) || !source.includes(CLASS_LINE)) {
      throw new Error("withTopRefreshRate: MainActivity has an unknown shape.");
    }
    source = source.replace(ON_CREATE, `${ON_CREATE}    ${MARKER}()\n`);
    source = source.replace(CLASS_LINE, `${CLASS_LINE}${METHOD}`);
    activity.modResults.contents = source;
    return activity;
  });
};
