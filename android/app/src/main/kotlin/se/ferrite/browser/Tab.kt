package se.ferrite.browser

import android.graphics.Bitmap
import android.webkit.WebView
import androidx.webkit.ScriptHandler
import org.json.JSONArray
import org.json.JSONObject
import java.util.concurrent.atomic.AtomicInteger

class Tab(val id: Long, var url: String = "", var title: String = "") {
    var webView: WebView? = null
    var favicon: Bitmap? = null
    var desktop = false
    var progress = 0
    var loading = false

    /** Tab that opened this one (back closes the tab and returns there). */
    var openerId: Long = -1

    /**
     * The page the tab shows now: the first party for ad blocking. Written on the UI thread and in
     * shouldInterceptRequest for main-frame requests, read on WebView's IO threads.
     */
    @Volatile var topUrl: String = ""

    /** Requests blocked on this page. */
    val blocked = AtomicInteger()

    var scriptHandler: ScriptHandler? = null
    var scriptKey: String? = null

    /** A page was loaded (or is being restored); otherwise the native new-tab page shows. */
    var hasPage = false
    val isBlank: Boolean get() = !hasPage
    val displayTitle: String
        get() = title.ifEmpty { if (isBlank) "New tab" else url }
}

/** Saved tabs (URLs only) and the selected one, as JSON in preferences. */
object Session {
    private const val KEY = "session"

    fun save(settings: Settings, tabs: List<Tab>, current: Int) {
        val arr = JSONArray()
        for (t in tabs) {
            arr.put(JSONObject().put("url", t.url).put("title", t.title).put("desktop", t.desktop))
        }
        settings.prefs.edit().putString(KEY, JSONObject().put("tabs", arr).put("current", current).toString()).apply()
    }

    class Saved(val tabs: List<Triple<String, String, Boolean>>, val current: Int)

    fun load(settings: Settings): Saved? = try {
        val o = JSONObject(settings.prefs.getString(KEY, null) ?: return null)
        val arr = o.getJSONArray("tabs")
        val tabs = (0 until arr.length()).map {
            val t = arr.getJSONObject(it)
            Triple(t.getString("url"), t.optString("title"), t.optBoolean("desktop"))
        }
        if (tabs.isEmpty()) null else Saved(tabs, o.optInt("current").coerceIn(0, tabs.size - 1))
    } catch (_: Exception) {
        null
    }
}
