package se.ferrite.browser

import android.app.Activity
import android.content.ActivityNotFoundException
import android.content.Intent
import android.net.Uri
import androidx.appcompat.app.AlertDialog

/** Links to other apps (intent:, tel:, mailto:, market: ...). They never start on their own: the user must have tapped and then confirms. */
object ExternalLinks {
    private val simple = setOf("tel", "mailto", "sms", "smsto", "geo", "market", "sip", "facetime")

    /** Handle a non-web top-level navigation. Returns true if the navigation is consumed (always, for these schemes). */
    fun handle(activity: Activity, url: String, hasGesture: Boolean, openInTab: (String) -> Unit): Boolean {
        val scheme = url.substringBefore(':', "").lowercase()
        if (scheme == "intent") {
            if (!hasGesture) return true
            val intent = try {
                Intent.parseUri(url, Intent.URI_INTENT_SCHEME)
            } catch (_: Exception) {
                return true
            }
            sanitize(intent)
            val fallback = intent.getStringExtra("browser_fallback_url")
            confirm(activity, intent.`package` ?: intent.dataString ?: "another app", intent) {
                if (fallback != null && Urls.isWeb(fallback)) openInTab(fallback)
            }
        } else if (scheme in simple) {
            if (!hasGesture) return true
            confirm(activity, url, Intent(Intent.ACTION_VIEW, Uri.parse(url)).addCategory(Intent.CATEGORY_BROWSABLE)) {}
        }
        // Everything else (javascript:, file:, content:, ...) is dropped.
        return true
    }

    /** Remove what a web page must not control: explicit components, selectors, file grants. */
    private fun sanitize(intent: Intent) {
        intent.component = null
        intent.selector = null
        intent.sourceBounds = null
        intent.addCategory(Intent.CATEGORY_BROWSABLE)
        intent.flags = intent.flags and (Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION or
            Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION or Intent.FLAG_GRANT_PREFIX_URI_PERMISSION).inv()
    }

    private fun confirm(activity: Activity, what: String, intent: Intent, onFail: () -> Unit) {
        AlertDialog.Builder(activity)
            .setTitle("Open in another app?")
            .setMessage(what.take(200))
            .setPositiveButton("Open") { _, _ ->
                try {
                    activity.startActivity(intent)
                } catch (_: ActivityNotFoundException) {
                    onFail()
                } catch (_: SecurityException) {
                    onFail()
                }
            }
            .setNegativeButton("Cancel", null)
            .show()
    }
}
