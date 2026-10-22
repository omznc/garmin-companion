// The entry file. The background task must be defined when the JavaScript starts, also in the
// headless context that the system starts for it. The router entry alone does not load it.
import "./src/notify/backgroundSync";
import "expo-router/entry";
