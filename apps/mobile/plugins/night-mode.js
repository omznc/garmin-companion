// Config plugin for modules/night-mode.
// It sets the saved light or dark choice before the activity starts, so the first
// frame has the right window background. AppCompat fixes the night mode in
// attachBaseContext, and the JavaScript loads later. expo-system-ui sets the mode of
// "userInterfaceStyle" in onCreate, so the choice goes back after super.onCreate.
const { withMainActivity } = require("expo/config-plugins");

const NIGHT_MODE = "expo.modules.nightmode.NightMode";
const ON_CREATE = "    super.onCreate(null)\n";
const CLASS_LINE = "class MainActivity : ReactActivity() {\n";

module.exports = function withNightMode(config) {
  return withMainActivity(config, (activity) => {
    if (activity.modResults.language !== "kt") {
      throw new Error("withNightMode: MainActivity must be Kotlin.");
    }
    let source = activity.modResults.contents;
    if (source.includes(NIGHT_MODE)) return activity;
    if (!source.includes(ON_CREATE) || !source.includes(CLASS_LINE)) {
      throw new Error("withNightMode: MainActivity has an unknown shape.");
    }
    source = source.replace(ON_CREATE, `${ON_CREATE}    ${NIGHT_MODE}.apply(this)\n`);
    source = source.replace(
      CLASS_LINE,
      `${CLASS_LINE}  override fun attachBaseContext(newBase: android.content.Context) {\n` +
        `    ${NIGHT_MODE}.apply(newBase)\n` +
        "    super.attachBaseContext(newBase)\n" +
        "  }\n\n",
    );
    activity.modResults.contents = source;
    return activity;
  });
};
