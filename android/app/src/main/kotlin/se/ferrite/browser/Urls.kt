package se.ferrite.browser

import java.net.URI
import java.net.URLEncoder
import java.util.Locale

/** Search engines the address bar can use. Google is the default, as on the desktop. */
enum class SearchEngine(val id: String, val label: String, val template: String) {
    GOOGLE("google", "Google", "https://www.google.com/search?q=%s"),
    DUCKDUCKGO("duckduckgo", "DuckDuckGo", "https://duckduckgo.com/?q=%s"),
    BING("bing", "Bing", "https://www.bing.com/search?q=%s"),
    STARTPAGE("startpage", "Startpage", "https://www.startpage.com/do/search?q=%s");

    fun searchUrl(query: String): String = template.replace("%s", URLEncoder.encode(query, "UTF-8"))

    companion object {
        fun byId(id: String?): SearchEngine = entries.firstOrNull { it.id == id } ?: GOOGLE
    }
}

/** Pure URL helpers (no Android classes, so they run in plain unit tests). */
object Urls {
    /** Only these can be loaded as a top-level page. */
    fun isAllowedTopLevel(url: String): Boolean {
        val u = url.trim()
        return u.startsWith("https://", true) || u.startsWith("http://", true) ||
            u.equals("about:blank", true) || u.startsWith("data:", true)
    }

    fun isWeb(url: String): Boolean =
        url.startsWith("https://", true) || url.startsWith("http://", true)

    private fun host(url: String): String? = try {
        URI(url).host?.lowercase(Locale.ROOT)?.takeIf { it.isNotEmpty() }
    } catch (_: Exception) {
        null
    }

    /** Lowercase host without a leading "www." (null for other schemes). Matches the desktop `site_key`. */
    fun siteKey(url: String): String? {
        if (!isWeb(url)) return null
        val h = host(url) ?: return null
        return h.removePrefix("www.").takeIf { it.isNotEmpty() }
    }

    /** Normalise a user-supplied exception ("WWW.Example.com", a full URL); null if it names no host. */
    fun normalizeSite(input: String): String? {
        val t = input.trim()
        if (t.contains("://")) return siteKey(t)
        return siteKey("https://$t")?.takeIf { !t.contains('/') || it.length > 1 }
    }

    /** True if [url]'s site is an exception or a subdomain of one (dot boundary). */
    fun isException(exceptions: Collection<String>, url: String): Boolean {
        if (exceptions.isEmpty()) return false
        val key = siteKey(url) ?: return false
        return exceptions.any { e -> key == e || key.endsWith(".$e") }
    }

    /** `scheme://host[:port]` of an http(s) URL, the form `addDocumentStartJavaScript` wants. */
    fun origin(url: String): String? {
        if (!isWeb(url)) return null
        return try {
            val u = URI(url)
            val scheme = u.scheme.lowercase(Locale.ROOT)
            val host = u.host?.lowercase(Locale.ROOT) ?: return null
            val hostPart = if (host.contains(':')) "[$host]" else host
            val default = if (scheme == "https") 443 else 80
            if (u.port == -1 || u.port == default) "$scheme://$hostPart" else "$scheme://$hostPart:${u.port}"
        } catch (_: Exception) {
            null
        }
    }

    private val hostLike = Regex("^[a-z0-9]([a-z0-9-]*[a-z0-9])?(\\.[a-z0-9]([a-z0-9-]*[a-z0-9])?)+(:\\d+)?([/?#].*)?$", RegexOption.IGNORE_CASE)
    private val localLike = Regex("^(localhost|\\[[0-9a-f:]+]|\\d{1,3}(\\.\\d{1,3}){3})(:\\d+)?([/?#].*)?$", RegexOption.IGNORE_CASE)

    /** What the address bar's text means: a URL to load, or a search. */
    fun resolveInput(input: String, engine: SearchEngine): String {
        val t = input.trim()
        if (t.isEmpty()) return "about:blank"
        if (t.equals("about:blank", true)) return t
        if (isWeb(t) && !t.any { it.isWhitespace() }) return t
        if (!t.any { it.isWhitespace() }) {
            if (localLike.matches(t)) return "http://$t"
            if (hostLike.matches(t)) return "https://$t"
        }
        return engine.searchUrl(t)
    }

    // --- update check (port of core/src/update.rs, versions only) ---

    private data class Version(val nums: List<Long>, val pre: List<String>)

    private fun parseVersion(s: String): Version? {
        val core = s.trim().substringBefore('+')
        val numPart = core.substringBefore('-')
        val pre = if (core.contains('-')) core.substringAfter('-').split('.') else emptyList()
        val parts = numPart.split('.')
        if (parts.size != 3 || parts.any { it.isEmpty() || !it.all(Char::isDigit) }) return null
        if (pre.any { it.isEmpty() }) return null
        return Version(parts.map { it.toLongOrNull() ?: return null }, pre)
    }

    /** True if [candidate] is a valid version greater than the valid [current]. */
    fun isNewer(candidate: String, current: String): Boolean {
        val c = parseVersion(candidate) ?: return false
        val cur = parseVersion(current) ?: return false
        for (i in 0..2) if (c.nums[i] != cur.nums[i]) return c.nums[i] > cur.nums[i]
        return when {
            c.pre.isEmpty() && cur.pre.isEmpty() -> false
            c.pre.isEmpty() -> true // a release is newer than its pre-releases
            cur.pre.isEmpty() -> false
            else -> comparePre(c.pre, cur.pre) > 0
        }
    }

    private fun comparePre(a: List<String>, b: List<String>): Int {
        for (i in 0 until minOf(a.size, b.size)) {
            val x = a[i]
            val y = b[i]
            val xn = x.all(Char::isDigit)
            val yn = y.all(Char::isDigit)
            val r = when {
                xn && yn -> x.toLong().compareTo(y.toLong())
                xn -> -1
                yn -> 1
                else -> x.compareTo(y)
            }
            if (r != 0) return r
        }
        return a.size.compareTo(b.size)
    }
}
