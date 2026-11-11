package se.ferrite.browser

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class UrlsTest {
    @Test fun siteKeys() {
        assertEquals("example.com", Urls.siteKey("https://WWW.Example.com/a?b"))
        assertEquals("sub.example.com", Urls.siteKey("http://sub.example.com:8080/"))
        assertEquals("localhost", Urls.siteKey("http://localhost:8769/x"))
        assertNull(Urls.siteKey("file:///tmp/a.html"))
        assertNull(Urls.siteKey("about:blank"))
        assertNull(Urls.siteKey("garbage"))
    }

    @Test fun exceptionsMatchOnDotBoundary() {
        val ex = listOf("example.com")
        assertTrue(Urls.isException(ex, "https://example.com/"))
        assertTrue(Urls.isException(ex, "https://www.example.com/x"))
        assertTrue(Urls.isException(ex, "https://a.b.example.com/"))
        assertFalse(Urls.isException(ex, "https://badexample.com/"))
        assertFalse(Urls.isException(ex, "https://example.com.evil.net/"))
        assertFalse(Urls.isException(ex, "ftp://example.com/"))
        assertFalse(Urls.isException(emptyList(), "https://example.com/"))
    }

    @Test fun normalizeSites() {
        assertEquals("example.com", Urls.normalizeSite(" WWW.Example.com "))
        assertEquals("a.org", Urls.normalizeSite("https://www.a.org/p"))
        assertNull(Urls.normalizeSite("ferrite://x"))
        assertNull(Urls.normalizeSite(""))
    }

    @Test fun origins() {
        assertEquals("https://www.youtube.com", Urls.origin("https://www.youtube.com/watch?v=1"))
        assertEquals("http://127.0.0.1:8080", Urls.origin("http://127.0.0.1:8080/x"))
        assertEquals("https://a.org", Urls.origin("https://a.org:443/"))
        assertNull(Urls.origin("about:blank"))
        assertNull(Urls.origin("javascript:alert(1)"))
    }

    @Test fun topLevelSchemes() {
        assertTrue(Urls.isAllowedTopLevel("https://a.org"))
        assertTrue(Urls.isAllowedTopLevel("about:blank"))
        assertTrue(Urls.isAllowedTopLevel("data:text/html,hi"))
        assertFalse(Urls.isAllowedTopLevel("javascript:alert(1)"))
        assertFalse(Urls.isAllowedTopLevel("file:///etc/passwd"))
        assertFalse(Urls.isAllowedTopLevel("intent://x#Intent;end"))
        assertFalse(Urls.isAllowedTopLevel("content://x"))
    }

    @Test fun addressBarInput() {
        val g = SearchEngine.GOOGLE
        assertEquals("https://example.com", Urls.resolveInput("https://example.com", g))
        assertEquals("https://example.com/a", Urls.resolveInput("example.com/a", g))
        assertEquals("http://localhost:3000", Urls.resolveInput("localhost:3000", g))
        assertEquals("http://192.168.0.1", Urls.resolveInput("192.168.0.1", g))
        assertEquals("https://www.google.com/search?q=hello+world", Urls.resolveInput("hello world", g))
        assertEquals("https://duckduckgo.com/?q=cats", Urls.resolveInput("cats", SearchEngine.DUCKDUCKGO))
        assertTrue(Urls.resolveInput("javascript:alert(1)", g).startsWith("https://www.google.com/search"))
        assertEquals("about:blank", Urls.resolveInput("  ", g))
    }

    @Test fun versions() {
        assertTrue(Urls.isNewer("0.2.0", "0.1.9"))
        assertTrue(Urls.isNewer("1.0.0", "1.0.0-rc.1"))
        assertFalse(Urls.isNewer("1.0.0-rc.1", "1.0.0"))
        assertFalse(Urls.isNewer("0.1.0", "0.1.0"))
        assertFalse(Urls.isNewer("x", "0.1.0"))
        assertTrue(Urls.isNewer("0.10.0", "0.9.0"))
        assertTrue(Urls.isNewer("1.0.0-rc.10", "1.0.0-rc.2"))
    }
}
