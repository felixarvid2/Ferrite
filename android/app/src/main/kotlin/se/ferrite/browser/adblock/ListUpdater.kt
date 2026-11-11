package se.ferrite.browser.adblock

import android.util.Log
import java.io.File
import java.net.HttpURLConnection
import java.net.URL

/** Downloads the filter lists with the platform TLS stack; a daily refresh like the desktop. */
object ListUpdater {
    private const val TAG = "ListUpdater"
    const val MAX_AGE_MS = 24L * 3600 * 1000
    private const val MAX_BYTES = 20 * 1024 * 1024

    /** Sanity check: real lists are large and start with a header (`[Adblock Plus 2.0]` or `! Title:`). */
    fun looksValid(text: String): Boolean = text.length > 1000 && (text.startsWith("[") || text.startsWith("!"))

    /** Returns how many lists were updated. */
    fun update(dir: File, force: Boolean): Int {
        dir.mkdirs()
        var updated = 0
        for (l in FILTER_LISTS) {
            val f = File(dir, l.name)
            if (!force && f.exists() && System.currentTimeMillis() - f.lastModified() < MAX_AGE_MS) continue
            try {
                val text = download(l.url)
                if (!looksValid(text)) {
                    Log.w(TAG, "${l.url} looks invalid; keeping the old copy")
                    continue
                }
                val tmp = File(dir, l.name + ".tmp")
                tmp.writeText(text)
                if (!tmp.renameTo(f)) {
                    f.delete()
                    tmp.renameTo(f)
                }
                updated++
            } catch (e: Exception) {
                Log.w(TAG, "downloading ${l.url} failed: ${e.message}")
            }
        }
        return updated
    }

    private fun download(url: String): String {
        val c = URL(url).openConnection() as HttpURLConnection
        try {
            c.connectTimeout = 15_000
            c.readTimeout = 60_000
            c.setRequestProperty("User-Agent", "Ferrite-Android")
            if (c.responseCode != 200) error("HTTP ${c.responseCode}")
            val out = java.io.ByteArrayOutputStream()
            c.inputStream.use { input ->
                val buf = ByteArray(64 * 1024)
                while (true) {
                    val n = input.read(buf)
                    if (n < 0) break
                    out.write(buf, 0, n)
                    if (out.size() > MAX_BYTES) error("list too large")
                }
            }
            return out.toString("UTF-8")
        } finally {
            c.disconnect()
        }
    }
}
