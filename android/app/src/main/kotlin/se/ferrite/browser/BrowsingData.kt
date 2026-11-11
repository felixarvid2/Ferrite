package se.ferrite.browser

import android.content.Context
import android.webkit.CookieManager
import android.webkit.WebStorage
import android.webkit.WebView

/** "Clear browsing data". */
object BrowsingData {
    val labels = arrayOf("Browsing history", "Cookies and site data", "Cached files", "Site permissions (location, camera, microphone)")

    fun clear(ctx: Context, which: BooleanArray) {
        val app = ctx.app
        if (which[0]) app.db.clearHistory()
        if (which[1]) {
            CookieManager.getInstance().removeAllCookies(null)
            CookieManager.getInstance().flush()
            WebStorage.getInstance().deleteAllData()
        }
        if (which[2]) WebView(ctx).apply {
            clearCache(true)
            destroy()
        }
        if (which[3]) app.db.clearPermissions()
    }
}
