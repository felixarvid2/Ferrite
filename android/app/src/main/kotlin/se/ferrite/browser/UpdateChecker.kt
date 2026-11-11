package se.ferrite.browser

import org.json.JSONArray
import java.net.HttpURLConnection
import java.net.URL

/** Asks GitHub once a day whether a newer `ferrite-android-v*` release exists. Nothing is downloaded or installed. */
object UpdateChecker {
    const val TAG_PREFIX = "ferrite-android-v"
    const val INTERVAL_MS = 24L * 3600 * 1000
    private const val MAX_BODY = 1024 * 1024

    data class Release(val version: String, val url: String)

    fun isDue(last: Long, now: Long): Boolean = last <= 0 || now < last || now - last >= INTERVAL_MS

    /** Newest published `ferrite-android-v*` release newer than [current]; the link is the APK if the release has one, else its page. */
    fun newest(json: String, current: String): Release? {
        val arr = JSONArray(json)
        var best: Release? = null
        for (i in 0 until arr.length()) {
            val r = arr.optJSONObject(i) ?: continue
            if (r.optBoolean("draft") || r.optBoolean("prerelease")) continue
            val tag = r.optString("tag_name")
            if (!tag.startsWith(TAG_PREFIX)) continue
            val version = tag.removePrefix(TAG_PREFIX)
            if (!Urls.isNewer(version, current)) continue
            if (best != null && !Urls.isNewer(version, best.version)) continue
            var link = r.optString("html_url")
            val assets = r.optJSONArray("assets")
            if (assets != null) {
                for (j in 0 until assets.length()) {
                    val a = assets.optJSONObject(j) ?: continue
                    if (a.optString("name").endsWith(".apk")) link = a.optString("browser_download_url", link)
                }
            }
            if (!Urls.isWeb(link) || !link.startsWith("https://")) continue
            best = Release(version, link)
        }
        return best
    }

    /** Network call; run off the UI thread. */
    fun fetch(repo: String, current: String): Release? {
        val c = URL("https://api.github.com/repos/$repo/releases?per_page=20").openConnection() as HttpURLConnection
        try {
            c.connectTimeout = 15_000
            c.readTimeout = 20_000
            c.setRequestProperty("User-Agent", "Ferrite-Android/$current")
            c.setRequestProperty("Accept", "application/vnd.github+json")
            if (c.responseCode != 200) error("HTTP ${c.responseCode}")
            val body = c.inputStream.use { it.readNBytes(MAX_BODY) }
            return newest(String(body, Charsets.UTF_8), current)
        } finally {
            c.disconnect()
        }
    }
}
