package se.ferrite.browser

import android.app.Application
import se.ferrite.browser.adblock.AdBlocker
import kotlin.concurrent.thread

class FerriteApp : Application() {
    lateinit var settings: Settings
        private set
    lateinit var db: Db
        private set

    override fun onCreate() {
        super.onCreate()
        settings = Settings(this)
        db = Db(this)
        AdBlocker.start(this, download = true)
    }

    /** Daily update check (setting, default on); at most one request a day. */
    fun checkForUpdate(force: Boolean = false, done: (() -> Unit)? = null) {
        if (!settings.updateCheck && !force) return
        val now = System.currentTimeMillis()
        if (!force && !UpdateChecker.isDue(settings.lastUpdateCheck, now)) return
        settings.lastUpdateCheck = now
        thread(name = "update-check", isDaemon = true) {
            try {
                val r = UpdateChecker.fetch(BuildConfig.UPDATE_REPO, BuildConfig.VERSION_NAME)
                settings.update = r?.let { it.version to it.url }
            } catch (_: Exception) {
                // Offline or rate limited: try again tomorrow.
            }
            done?.invoke()
        }
    }

    /** Refresh filter lists if older than a day (engine is rebuilt in the background). */
    fun refreshLists(force: Boolean = false) = AdBlocker.start(this, force = force)
}

val android.content.Context.app: FerriteApp get() = applicationContext as FerriteApp
