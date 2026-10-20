package expo.modules.nightmode

import android.content.Context
import expo.modules.kotlin.exception.Exceptions
import expo.modules.kotlin.modules.Module
import expo.modules.kotlin.modules.ModuleDefinition

/** Gives JavaScript the `save` of `NightMode`. */
class NightModeModule : Module() {
  private val context: Context
    get() = appContext.reactContext?.applicationContext ?: throw Exceptions.ReactContextLost()

  override fun definition() = ModuleDefinition {
    Name("NightMode")

    Function("save") { scheme: String ->
      NightMode.save(context, scheme)
    }
  }
}
