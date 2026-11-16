package page.fibo.alerts

import android.content.Intent
import android.content.pm.ServiceInfo
import android.util.Log
import androidx.core.app.ServiceCompat
import com.facebook.react.HeadlessJsTaskService
import com.facebook.react.bridge.Arguments
import com.facebook.react.jstasks.HeadlessJsTaskConfig

/**
 * Keeps this process and its JS (the hub and its encrypted connections) running with the app closed.
 * `remoteMessaging`, not `dataSync`: Android 15 stops a dataSync service after 6 hours in 24.
 */
class KeepConnectedService : HeadlessJsTaskService() {
  private var running = false

  override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
    try {
      ServiceCompat.startForeground(
        this,
        Notifications.SERVICE_ID,
        Notifications.service(this),
        ServiceInfo.FOREGROUND_SERVICE_TYPE_REMOTE_MESSAGING,
      )
    } catch (e: RuntimeException) {
      // Brought back by the system when it may no longer be in the foreground: the next time the
      // app opens starts it again. Nothing else can be done from here.
      Log.w("FiboAlerts", "not allowed in the foreground", e)
      stopSelf()
      return START_NOT_STICKY
    }
    // Every start would otherwise add a task that never ends.
    if (!running) {
      running = true
      super.onStartCommand(intent, flags, startId)
    }
    return START_STICKY
  }

  // No timeout: the task lasts until the switch is turned off. Allowed with the app in front,
  // which is where it starts.
  override fun getTaskConfig(intent: Intent?) = HeadlessJsTaskConfig(TASK, Arguments.createMap(), 0, true)

  private companion object {
    /** Same name in `src/remote/background.ts`. */
    const val TASK = "FiboKeepConnected"
  }
}
