package se.ferrite.browser

import android.graphics.Bitmap
import android.net.http.SslError
import android.os.Message
import android.view.View
import android.webkit.ConsoleMessage
import android.webkit.GeolocationPermissions
import android.webkit.PermissionRequest
import android.webkit.RenderProcessGoneDetail
import android.webkit.SslErrorHandler
import android.webkit.ValueCallback
import android.webkit.WebChromeClient
import android.webkit.WebResourceError
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebView
import android.webkit.WebViewClient
import se.ferrite.browser.adblock.AdBlocker
import java.io.ByteArrayInputStream

/** Page events of one tab: navigation policy, ad blocking of subresources, error pages. */
class TabClient(private val a: BrowserActivity, private val tab: Tab) : WebViewClient() {

    override fun shouldOverrideUrlLoading(view: WebView, request: WebResourceRequest): Boolean {
        val url = request.url.toString()
        if (!request.isForMainFrame) {
            // Frames may load web pages only; apps are never started from a frame.
            return !Urls.isWeb(url) && !url.equals("about:blank", true) && !url.startsWith("about:srcdoc")
        }
        if (Urls.isWeb(url) || url.equals("about:blank", true)) {
            if (Urls.isWeb(url)) {
                tab.topUrl = url
                DocumentStart.prepare(tab, url, a.settings)
            }
            return false
        }
        ExternalLinks.handle(a, url, request.hasGesture()) { a.openUrl(it) }
        return true
    }

    override fun shouldInterceptRequest(view: WebView, request: WebResourceRequest): WebResourceResponse? {
        val url = request.url.toString()
        if (request.isForMainFrame) {
            if (Urls.isWeb(url)) tab.topUrl = url
            return null
        }
        if (!Urls.isWeb(url)) return null
        val top = tab.topUrl
        if (top.isEmpty() || !a.settings.blockingFor(top)) return null
        val type = requestType(request)
        if (!AdBlocker.shouldBlock(url, top, type)) return null
        tab.blocked.incrementAndGet()
        a.blockedChanged(tab)
        // A failed response (not 200) makes scripts, styles and images fire `error` like a blocked request.
        return WebResourceResponse(mimeFor(type), "utf-8", 403, "Blocked", mapOf("Cache-Control" to "no-store"), ByteArrayInputStream(ByteArray(0)))
    }

    override fun onPageStarted(view: WebView, url: String, favicon: Bitmap?) {
        tab.loading = true
        if (Urls.isWeb(url)) {
            tab.url = url
            tab.topUrl = url
            tab.hasPage = true
            tab.blocked.set(0)
            DocumentStart.prepare(tab, url, a.settings)
        }
        a.tabChanged(tab)
    }

    override fun onPageFinished(view: WebView, url: String) {
        tab.loading = false
        a.tabChanged(tab)
        a.saveSession()
    }

    override fun doUpdateVisitedHistory(view: WebView, url: String, isReload: Boolean) {
        if (Urls.isWeb(url)) {
            tab.url = url
            if (!isReload) a.db.addVisit(url, view.title.orEmpty())
        }
        a.tabChanged(tab)
    }

    override fun onReceivedError(view: WebView, request: WebResourceRequest, error: WebResourceError) {
        if (!request.isForMainFrame) return
        // ERR_ABORTED-style cancellations (-3 is ERROR_UNKNOWN for downloads and handled navigations) show nothing.
        if (error.errorCode == ERROR_UNKNOWN) return
        showErrorPage(view, request.url.toString(), error.description?.toString() ?: "The page could not be loaded.")
    }

    // Certificate errors are never bypassed.
    override fun onReceivedSslError(view: WebView, handler: SslErrorHandler, error: SslError) {
        handler.cancel()
        if (error.url == tab.topUrl) {
            showErrorPage(view, error.url, "The site's security certificate is not trusted, so Ferrite did not connect.")
        }
    }

    override fun onRenderProcessGone(view: WebView, detail: RenderProcessGoneDetail): Boolean {
        a.recreateWebView(tab)
        return true
    }

    private fun showErrorPage(view: WebView, url: String, message: String) {
        val esc = { s: String -> s.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;").replace("\"", "&quot;") }
        val html = """<!doctype html><html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
            <meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline'">
            <title>Can't open page</title><style>
            body{font-family:sans-serif;margin:0;padding:32px 24px;background:#fff;color:#1b1b1f}
            @media(prefers-color-scheme:dark){body{background:#121214;color:#e4e1e6}a{color:#b7c1ff}}
            h1{font-size:22px}p{line-height:1.5;word-break:break-word}</style></head><body>
            <h1>This page can't be opened</h1><p>${esc(message)}</p><p>${esc(url)}</p>
            <p><a href="${esc(url)}">Try again</a></p></body></html>"""
        view.loadDataWithBaseURL(url, html, "text/html", "utf-8", url)
    }

    companion object {
        private val script = setOf("js", "mjs")
        private val image = setOf("png", "jpg", "jpeg", "gif", "webp", "svg", "ico", "avif", "bmp")
        private val font = setOf("woff", "woff2", "ttf", "otf", "eot")
        private val media = setOf("mp4", "webm", "m4a", "mp3", "ogg", "m3u8", "mpd", "ts", "m4s")

        /**
         * WebResourceRequest does not say what kind of resource it is, so guess it from the file
         * extension and the Accept header; unknown requests are "other" (matched by rules that name no type).
         */
        fun requestType(r: WebResourceRequest): String {
            val ext = r.url.lastPathSegment?.substringAfterLast('.', "")?.lowercase().orEmpty()
            val accept = r.requestHeaders["Accept"].orEmpty()
            return when {
                ext in script -> "script"
                ext == "css" -> "stylesheet"
                ext in image -> "image"
                ext in font -> "font"
                ext in media -> "media"
                accept.startsWith("text/css") -> "stylesheet"
                accept.startsWith("image/") -> "image"
                accept.contains("text/html") -> "sub_frame"
                else -> "other"
            }
        }

        private fun mimeFor(type: String) = when (type) {
            "script" -> "application/javascript"
            "stylesheet" -> "text/css"
            "image" -> "image/gif"
            "sub_frame" -> "text/html"
            else -> "text/plain"
        }
    }
}

/** Browser chrome callbacks of one tab: progress, title, icons, dialogs, permissions, fullscreen, popups. */
class TabChromeClient(private val a: BrowserActivity, private val tab: Tab) : WebChromeClient() {
    override fun onProgressChanged(view: WebView, newProgress: Int) {
        tab.progress = newProgress
        a.tabChanged(tab)
    }

    override fun onReceivedTitle(view: WebView, title: String) {
        tab.title = title
        a.db.updateTitle(tab.url, title)
        a.tabChanged(tab)
    }

    override fun onReceivedIcon(view: WebView, icon: Bitmap) {
        tab.favicon = icon
        a.tabChanged(tab)
    }

    override fun onShowFileChooser(webView: WebView, callback: ValueCallback<Array<android.net.Uri>>, params: FileChooserParams): Boolean =
        a.showFileChooser(callback, params)

    override fun onShowCustomView(view: View, callback: CustomViewCallback) = a.showCustomView(view, callback)

    override fun onHideCustomView() = a.hideCustomView()

    override fun onPermissionRequest(request: PermissionRequest) = a.onWebPermissionRequest(request)

    override fun onGeolocationPermissionsShowPrompt(origin: String, callback: GeolocationPermissions.Callback) =
        a.onGeolocationRequest(origin, callback)

    override fun onCreateWindow(view: WebView, isDialog: Boolean, isUserGesture: Boolean, resultMsg: Message): Boolean {
        // Pop-ups without a user gesture are dropped.
        if (!isUserGesture) return false
        val popup = a.newTab("", select = true, opener = tab.id)
        popup.hasPage = true
        val wv = a.ensureWebView(popup)
        (resultMsg.obj as WebView.WebViewTransport).webView = wv
        resultMsg.sendToTarget()
        return true
    }

    override fun onCloseWindow(window: WebView) {
        a.tabs.firstOrNull { it.webView === window }?.let { a.closeTab(it) }
    }

    override fun onConsoleMessage(m: ConsoleMessage): Boolean = true
}
