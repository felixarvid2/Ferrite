package se.ferrite.browser

import android.content.Context
import android.content.SharedPreferences
import androidx.preference.PreferenceManager

/** App settings in the default preferences file, which the settings screen edits directly. */
class Settings(ctx: Context) {
    val prefs: SharedPreferences = PreferenceManager.getDefaultSharedPreferences(ctx.applicationContext)

    var searchEngine: SearchEngine
        get() = SearchEngine.byId(prefs.getString(SEARCH_ENGINE, null))
        set(v) = prefs.edit().putString(SEARCH_ENGINE, v.id).apply()

    /** Empty = the native new-tab page. */
    val homepage: String get() = prefs.getString(HOMEPAGE, "")?.trim().orEmpty()

    // Read for every request on WebView's IO threads, so cached.
    @Volatile private var adblockCached = prefs.getBoolean(ADBLOCK, true)
    @Volatile private var exceptionsCached: Set<String> = prefs.getStringSet(EXCEPTIONS, emptySet()).orEmpty().toSet()
    private val listener = SharedPreferences.OnSharedPreferenceChangeListener { p, key ->
        when (key) {
            ADBLOCK -> adblockCached = p.getBoolean(ADBLOCK, true)
            EXCEPTIONS -> exceptionsCached = p.getStringSet(EXCEPTIONS, emptySet()).orEmpty().toSet()
        }
    }

    init {
        prefs.registerOnSharedPreferenceChangeListener(listener)
    }

    var adblock: Boolean
        get() = adblockCached
        set(v) {
            adblockCached = v
            prefs.edit().putBoolean(ADBLOCK, v).apply()
        }

    var exceptions: Set<String>
        get() = exceptionsCached
        set(v) {
            exceptionsCached = v
            prefs.edit().putStringSet(EXCEPTIONS, v).apply()
        }

    val updateCheck: Boolean get() = prefs.getBoolean(UPDATE_CHECK, true)

    var lastUpdateCheck: Long
        get() = prefs.getLong(LAST_UPDATE_CHECK, 0)
        set(v) = prefs.edit().putLong(LAST_UPDATE_CHECK, v).apply()

    /** Newest release known to be newer than this build: version and download URL. */
    var update: Pair<String, String>?
        get() {
            val v = prefs.getString(UPDATE_VERSION, null) ?: return null
            val u = prefs.getString(UPDATE_URL, null) ?: return null
            return if (Urls.isNewer(v, BuildConfig.VERSION_NAME)) v to u else null
        }
        set(p) = prefs.edit().apply {
            if (p == null) {
                remove(UPDATE_VERSION)
                remove(UPDATE_URL)
            } else {
                putString(UPDATE_VERSION, p.first)
                putString(UPDATE_URL, p.second)
            }
        }.apply()

    /** Blocking is on for [url]: enabled and the site is not an exception. */
    fun blockingFor(url: String): Boolean = adblock && !Urls.isException(exceptions, url)

    fun addException(site: String) {
        exceptions = exceptions + site
    }

    fun removeException(site: String) {
        exceptions = exceptions - site
    }

    companion object {
        const val SEARCH_ENGINE = "search_engine"
        const val HOMEPAGE = "homepage"
        const val ADBLOCK = "adblock"
        const val EXCEPTIONS = "adblock_exceptions"
        const val UPDATE_CHECK = "update_check"
        private const val LAST_UPDATE_CHECK = "last_update_check"
        private const val UPDATE_VERSION = "update_version"
        private const val UPDATE_URL = "update_url"
    }
}
