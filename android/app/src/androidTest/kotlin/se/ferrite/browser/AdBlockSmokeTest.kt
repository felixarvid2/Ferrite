package se.ferrite.browser

import android.content.Intent
import android.net.Uri
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.webkit.WebViewFeature
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import se.ferrite.browser.adblock.AdBlocker
import java.net.InetAddress
import java.net.ServerSocket
import java.util.concurrent.atomic.AtomicInteger

/**
 * Loads a local page in the real browser with a seeded filter list and checks that
 * the blocked subresource fails, the allowed one loads, and the uBO scriptlet (registered
 * with addDocumentStartJavaScript) ran before the page's own inline script.
 */
@RunWith(AndroidJUnit4::class)
class AdBlockSmokeTest {
    private val ctx = InstrumentationRegistry.getInstrumentation().targetContext
    private lateinit var server: ServerSocket
    private val blockedHits = AtomicInteger()
    private val okHits = AtomicInteger()

    private val page = """<!doctype html><html><head><title>pending</title></head><body>
        <script>var before = String(window.ferriteMarker);</script>
        <script src="/ferrite-blocked.js" onerror="window.blockedFailed=true"></script>
        <script src="/ok.js"></script>
        <script>window.addEventListener('load', function () {
          document.title = 'done;marker=' + before + ';blockedFailed=' + !!window.blockedFailed +
            ';blockedRan=' + !!window.blockedRan + ';ok=' + !!window.okLoaded;
        });</script></body></html>"""

    @Before fun setUp() {
        assertTrue("libferrite_android.so is not in the APK", se.ferrite.browser.adblock.Native.loaded)
        assertTrue("WebView lacks DOCUMENT_START_SCRIPT", WebViewFeature.isFeatureSupported(WebViewFeature.DOCUMENT_START_SCRIPT))
        ctx.app.settings.prefs.edit().clear().commit()
        ctx.app.settings.exceptions = emptySet()
        ctx.app.settings.adblock = true
        assertTrue(
            AdBlocker.installForTest(
                ctx,
                "/ferrite-blocked.js\nlocalhost##+js(set-constant, ferriteMarker, true)\n",
            ),
        )
        server = ServerSocket(0, 10, InetAddress.getByName("127.0.0.1"))
        Thread {
            while (!server.isClosed) {
                try {
                    val s = server.accept()
                    Thread { serve(s) }.start()
                } catch (_: Exception) {
                    break
                }
            }
        }.apply { isDaemon = true }.start()
    }

    @After fun tearDown() {
        server.close()
    }

    private fun serve(s: java.net.Socket) = s.use {
        val path = it.getInputStream().bufferedReader().readLine().split(" ")[1]
        val out = it.getOutputStream()
        if (path == "/go") {
            // Cross-origin redirect: the script must be registered for the redirect target.
            out.write("HTTP/1.1 302 Found\r\nLocation: http://localhost:${server.localPort}/\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".toByteArray())
            out.flush()
            return@use
        }
        val (type, body) = when (path) {
            "/" -> "text/html" to page
            "/ok.js" -> { okHits.incrementAndGet(); "application/javascript" to "window.okLoaded=true;" }
            else -> { blockedHits.incrementAndGet(); "application/javascript" to "window.blockedRan=true;" }
        }
        val bytes = body.toByteArray()
        out.write("HTTP/1.1 200 OK\r\nContent-Type: $type\r\nContent-Length: ${bytes.size}\r\nConnection: close\r\n\r\n".toByteArray())
        out.write(bytes)
        out.flush()
    }

    private fun pageTitle(url: String, until: (String) -> Boolean): String {
        val intent = Intent(Intent.ACTION_VIEW, Uri.parse(url)).setClass(ctx, BrowserActivity::class.java)
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        ActivityScenario.launch<BrowserActivity>(intent).use { scenario ->
            var title = ""
            val end = System.currentTimeMillis() + 30_000
            while (System.currentTimeMillis() < end) {
                scenario.onActivity { a -> title = a.current?.webView?.title.orEmpty() }
                if (until(title)) break
                Thread.sleep(200)
            }
            return title
        }
    }

    @Test fun blocksSubresourceAndRunsScriptletBeforeInlineScript() {
        val url = "http://localhost:${server.localPort}/"
        val title = pageTitle(url) { it.startsWith("done;") }
        assertEquals("done;marker=true;blockedFailed=true;blockedRan=false;ok=true", title)
        assertEquals("blocked request must not reach the network", 0, blockedHits.get())
        assertTrue(okHits.get() >= 1)
    }

    @Test fun siteExceptionTurnsBlockingOff() {
        val port = server.localPort
        ctx.app.settings.addException("localhost")
        val title = pageTitle("http://localhost:$port/") { it.startsWith("done;") }
        assertEquals("done;marker=undefined;blockedFailed=false;blockedRan=true;ok=true", title)
    }

    @Test fun scriptletAppliesAfterCrossOriginRedirect() {
        val title = pageTitle("http://127.0.0.1:${server.localPort}/go") { it.startsWith("done;") }
        assertEquals("done;marker=true;blockedFailed=true;blockedRan=false;ok=true", title)
    }
}
