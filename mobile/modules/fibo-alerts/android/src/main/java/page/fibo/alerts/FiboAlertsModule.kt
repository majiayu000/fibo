package page.fibo.alerts

import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.PowerManager
import android.provider.Settings
import android.util.Log
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import expo.modules.kotlin.exception.Exceptions
import expo.modules.kotlin.modules.Module
import expo.modules.kotlin.modules.ModuleDefinition

class FiboAlertsModule : Module() {
  private val context: Context
    get() = appContext.reactContext ?: throw Exceptions.ReactContextLost()

  override fun definition() = ModuleDefinition {
    Name("FiboAlerts")

    Function("startKeepConnected") {
      try {
        ContextCompat.startForegroundService(context, Intent(context, KeepConnectedService::class.java))
        true
      } catch (e: RuntimeException) {
        // ForegroundServiceStartNotAllowedException: only with the app in front.
        Log.w("FiboAlerts", "service refused", e)
        false
      }
    }

    Function("stopKeepConnected") {
      context.stopService(Intent(context, KeepConnectedService::class.java))
      Unit
    }

    Function("show") { tag: String, title: String, body: String, quiet: Boolean ->
      Notifications.show(context, tag, title, body, quiet)
    }

    Function("hide") { tag: String ->
      Notifications.hide(context, tag)
    }

    Function("enabled") {
      NotificationManagerCompat.from(context).areNotificationsEnabled()
    }

    Function("batteryFree") {
      context.getSystemService(PowerManager::class.java).isIgnoringBatteryOptimizations(context.packageName)
    }

    Function("askBatteryFree") {
      val ask = Intent(Settings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS, Uri.parse("package:${context.packageName}"))
      try {
        // From the page on screen, so answering comes back here. FLAG_ACTIVITY_NEW_TASK would bring
        // the Settings task forward, and answering leaves the phone on whatever page it was left at.
        val page = appContext.throwingActivity
        page.startActivity(ask)
      } catch (e: Exception) {
        // No page on screen, or a system without this screen: the settings page still says what to do.
        Log.w("FiboAlerts", "no battery question here", e)
      }
    }
  }
}
