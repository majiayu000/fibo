package page.fibo.alerts

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.os.Build
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat

/** The two channels: what asks for you interrupts, the service's own line stays out of the way. */
internal object Notifications {
  // The ids a phone that ran the 11-07 trial build already has; a new id would leave those two
  // behind in the system's settings. Their names follow the phone's language (res/values*).
  private const val ASKS = "mooring-asks"
  private const val SERVICE = "mooring-service"
  const val SERVICE_ID = 1

  private fun channels(context: Context) {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
    val manager = context.getSystemService(NotificationManager::class.java)
    manager.createNotificationChannel(
      NotificationChannel(ASKS, context.getString(R.string.fibo_alerts_asks_channel), NotificationManager.IMPORTANCE_HIGH).apply {
        lockscreenVisibility = Notification.VISIBILITY_PRIVATE
      },
    )
    manager.createNotificationChannel(
      NotificationChannel(SERVICE, context.getString(R.string.fibo_alerts_service_channel), NotificationManager.IMPORTANCE_MIN).apply {
        setShowBadge(false)
      },
    )
  }

  private fun openApp(context: Context): PendingIntent? {
    val launch = context.packageManager.getLaunchIntentForPackage(context.packageName) ?: return null
    return PendingIntent.getActivity(context, 0, launch, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
  }

  fun service(context: Context): Notification {
    channels(context)
    return NotificationCompat.Builder(context, SERVICE)
      .setSmallIcon(R.drawable.fibo_alert)
      .setContentTitle(context.getString(R.string.fibo_alerts_service_title))
      .setContentIntent(openApp(context))
      .setOngoing(true)
      .setPriority(NotificationCompat.PRIORITY_MIN)
      .setCategory(NotificationCompat.CATEGORY_SERVICE)
      .build()
  }

  @Suppress("MissingPermission") // areNotificationsEnabled() below; a refused one is simply not shown.
  fun show(context: Context, tag: String, title: String, body: String, quiet: Boolean) {
    channels(context)
    val manager = NotificationManagerCompat.from(context)
    if (!manager.areNotificationsEnabled()) return
    val built = NotificationCompat.Builder(context, ASKS)
      .setSmallIcon(R.drawable.fibo_alert)
      .setContentTitle(title)
      .setContentText(body)
      .setContentIntent(openApp(context))
      .setAutoCancel(true)
      .setPriority(NotificationCompat.PRIORITY_HIGH)
      .setCategory(NotificationCompat.CATEGORY_MESSAGE)
      .setVisibility(NotificationCompat.VISIBILITY_PRIVATE)
      .setSilent(quiet)
      .build()
    manager.notify(tag, 0, built)
  }

  fun hide(context: Context, tag: String) {
    NotificationManagerCompat.from(context).cancel(tag, 0)
  }
}
