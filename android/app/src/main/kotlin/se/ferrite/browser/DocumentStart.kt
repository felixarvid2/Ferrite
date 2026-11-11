package se.ferrite.browser

import android.util.Log
import androidx.webkit.WebViewCompat
import androidx.webkit.WebViewFeature
import se.ferrite.browser.adblock.AdBlocker

/**
 * Registers the scriptlets and cosmetic CSS for a page with `addDocumentStartJavaScript`, which runs
 * them in the page's main world before any of the page's own scripts (the way the desktop browser uses
 * `Page.addScriptToEvaluateOnNewDocument`). Called on the UI thread before each navigation: our own
 * loads, link and redirect navigations (shouldOverrideUrlLoading) and page starts. The previous
 * registration is removed first. The script also checks `location.hostname`, so a stale registration
 * does nothing on another host.
 */
object DocumentStart {
    val supported: Boolean by lazy { WebViewFeature.isFeatureSupported(WebViewFeature.DOCUMENT_START_SCRIPT) }

    fun prepare(tab: Tab, url: String, settings: Settings) {
        val wv = tab.webView ?: return
        if (!supported) return
        val origin = Urls.origin(url)
        val enabled = origin != null && settings.blockingFor(url) && AdBlocker.ready
        val key = if (enabled) "$origin#${AdBlocker.generation.get()}" else null
        if (key == tab.scriptKey) return
        try {
            tab.scriptHandler?.remove()
            tab.scriptHandler = null
            tab.scriptKey = key
            if (key == null || origin == null) return
            val script = AdBlocker.cosmetic(url) ?: return
            tab.scriptHandler = WebViewCompat.addDocumentStartJavaScript(wv, script, setOf(origin))
        } catch (e: Exception) {
            Log.w("DocumentStart", "registering script failed", e)
        }
    }
}
