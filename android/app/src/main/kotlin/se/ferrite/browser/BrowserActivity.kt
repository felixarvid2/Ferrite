package se.ferrite.browser

import android.Manifest
import android.annotation.SuppressLint
import android.app.DownloadManager
import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.content.res.Configuration
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.os.Environment
import android.os.Handler
import android.os.Looper
import android.view.KeyEvent
import android.view.View
import android.view.ViewGroup
import android.view.WindowManager
import android.view.inputmethod.EditorInfo
import android.view.inputmethod.InputMethodManager
import android.webkit.CookieManager
import android.webkit.GeolocationPermissions
import android.webkit.PermissionRequest
import android.webkit.URLUtil
import android.webkit.ValueCallback
import android.webkit.WebChromeClient
import android.webkit.WebSettings
import android.webkit.WebView
import android.widget.AutoCompleteTextView
import android.widget.EditText
import android.widget.FrameLayout
import android.widget.ImageButton
import android.widget.PopupMenu
import android.widget.ProgressBar
import android.widget.TextView
import android.widget.Toast
import androidx.activity.OnBackPressedCallback
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.appcompat.app.AlertDialog
import androidx.appcompat.app.AppCompatActivity
import androidx.core.content.ContextCompat
import androidx.core.view.ViewCompat
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat
import androidx.swiperefreshlayout.widget.SwipeRefreshLayout
import androidx.webkit.WebSettingsCompat
import androidx.webkit.WebViewFeature
import se.ferrite.browser.adblock.AdBlocker

class BrowserActivity : AppCompatActivity() {
    val settings: Settings get() = app.settings
    val db: Db get() = app.db

    val tabs = mutableListOf<Tab>()
    var current: Tab? = null
        private set
    private var nextTabId = 1L

    private lateinit var root: FrameLayout
    private lateinit var main: View
    private lateinit var content: FrameLayout
    private lateinit var address: AutoCompleteTextView
    private lateinit var progress: ProgressBar
    private lateinit var refresh: SwipeRefreshLayout
    private lateinit var reloadButton: ImageButton
    private lateinit var shield: TextView
    private lateinit var tabsButton: TextView
    private lateinit var backButton: ImageButton
    private lateinit var forwardButton: ImageButton
    private lateinit var findBar: View
    private lateinit var findInput: EditText
    private lateinit var findCount: TextView
    private lateinit var newTabView: NewTabView

    private val ui = Handler(Looper.getMainLooper())
    private var customView: View? = null
    private var customCallback: WebChromeClient.CustomViewCallback? = null
    private var fileCallback: ValueCallback<Array<Uri>>? = null
    private var pendingDownload: (() -> Unit)? = null
    private var permissionCallback: ((Map<String, Boolean>) -> Unit)? = null

    private val fileLauncher = registerForActivityResult(ActivityResultContracts.StartActivityForResult()) { r ->
        val cb = fileCallback ?: return@registerForActivityResult
        fileCallback = null
        cb.onReceiveValue(WebChromeClient.FileChooserParams.parseResult(r.resultCode, r.data))
    }
    private val permissionLauncher = registerForActivityResult(ActivityResultContracts.RequestMultiplePermissions()) { r ->
        val cb = permissionCallback
        permissionCallback = null
        cb?.invoke(r)
    }
    private val libraryLauncher = registerForActivityResult(ActivityResultContracts.StartActivityForResult()) { r ->
        r.data?.getStringExtra(LibraryActivity.RESULT_URL)?.let { navigate(it) }
    }
    private val storageLauncher = registerForActivityResult(ActivityResultContracts.RequestPermission()) { ok ->
        val run = pendingDownload
        pendingDownload = null
        if (ok) run?.invoke() else toast("Storage permission is needed to download files.")
    }

    @SuppressLint("SetJavaScriptEnabled")
    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_browser)
        root = findViewById(R.id.root)
        main = findViewById(R.id.main)
        content = findViewById(R.id.content)
        address = findViewById(R.id.address)
        progress = findViewById(R.id.progress)
        refresh = findViewById(R.id.refresh)
        reloadButton = findViewById(R.id.reload)
        shield = findViewById(R.id.shield)
        tabsButton = findViewById(R.id.tabs)
        backButton = findViewById(R.id.back)
        forwardButton = findViewById(R.id.forward)
        findBar = findViewById(R.id.findbar)
        findInput = findViewById(R.id.find_input)
        findCount = findViewById(R.id.find_count)

        ViewCompat.setOnApplyWindowInsetsListener(main) { v, insets ->
            val bars = insets.getInsets(WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout())
            val ime = insets.getInsets(WindowInsetsCompat.Type.ime())
            v.setPadding(bars.left, bars.top, bars.right, maxOf(bars.bottom, ime.bottom))
            WindowInsetsCompat.CONSUMED
        }

        newTabView = NewTabView(this) { navigate(it) }
        setupAddressBar()
        setupButtons()

        refresh.setOnRefreshListener { current?.webView?.reload() ?: run { refresh.isRefreshing = false } }
        refresh.setOnChildScrollUpCallback { _, _ -> (current?.webView?.scrollY ?: 0) > 0 || current?.isBlank == true }

        onBackPressedDispatcher.addCallback(this, object : OnBackPressedCallback(true) {
            override fun handleOnBackPressed() = goBack()
        })

        AdBlocker.onChange = { ui.post { current?.let { t -> if (t.webView != null) DocumentStart.prepare(t, t.topUrl, settings) } } }

        restoreSession()
        handleIntent(intent, startup = true)
        app.checkForUpdate()
        app.refreshLists()
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        handleIntent(intent, startup = false)
    }

    override fun onResume() {
        super.onResume()
        current?.webView?.onResume()
        app.checkForUpdate()
        app.refreshLists()
        updateUi()
    }

    override fun onPause() {
        current?.webView?.onPause()
        saveSession()
        super.onPause()
    }

    override fun onDestroy() {
        AdBlocker.onChange = null
        saveSession()
        for (t in tabs) destroyWebView(t)
        super.onDestroy()
    }

    // --- intents and session ---

    private fun handleIntent(intent: Intent?, startup: Boolean) {
        val url = if (intent?.action == Intent.ACTION_VIEW) intent.dataString else null
        if (url != null && Urls.isWeb(url)) {
            // A link from another app opens in its own tab, unless it is the very first start with an empty tab.
            val cur = current
            if (startup && cur != null && cur.isBlank && tabs.size == 1) navigate(url) else newTab(url, select = true).also { load(it, url) }
        }
    }

    private fun restoreSession() {
        val saved = Session.load(settings)
        if (saved == null) {
            newTab(settings.homepage, select = true).also { if (settings.homepage.isNotEmpty()) load(it, settings.homepage) }
            return
        }
        for ((url, title, desktop) in saved.tabs) {
            val t = Tab(nextTabId++, url, title)
            t.desktop = desktop
            t.hasPage = url.isNotEmpty()
            t.topUrl = url
            tabs.add(t)
        }
        selectTab(tabs[saved.current])
    }

    fun saveSession() {
        if (tabs.isEmpty()) return
        Session.save(settings, tabs, tabs.indexOf(current).coerceAtLeast(0))
    }

    // --- tabs ---

    fun newTab(url: String = "", select: Boolean = true, opener: Long = -1): Tab {
        val t = Tab(nextTabId++, url)
        t.openerId = opener
        tabs.add(t)
        if (select) selectTab(t) else updateUi()
        return t
    }

    fun selectTab(tab: Tab) {
        current?.takeIf { it !== tab }?.let {
            it.webView?.onPause()
            if (it.webView?.parent === content) content.removeView(it.webView)
        }
        current = tab
        content.removeAllViews()
        findBar.visibility = View.GONE
        if (tab.isBlank) {
            newTabView.refresh(db)
            content.addView(newTabView.view)
        } else {
            val wv = ensureWebView(tab)
            if (wv.parent == null) content.addView(wv, ViewGroup.LayoutParams(-1, -1))
            wv.onResume()
        }
        updateUi()
        saveSession()
    }

    fun closeTab(tab: Tab) {
        val i = tabs.indexOf(tab)
        if (i < 0) return
        val wasCurrent = tab === current
        destroyWebView(tab)
        tabs.removeAt(i)
        if (tabs.isEmpty()) {
            current = null
            newTab(settings.homepage, select = true).also { if (settings.homepage.isNotEmpty()) load(it, settings.homepage) }
            return
        }
        if (wasCurrent) {
            current = null
            selectTab(tabs.firstOrNull { it.id == tab.openerId } ?: tabs[(i - 1).coerceAtLeast(0)])
        } else {
            updateUi()
            saveSession()
        }
    }

    private fun destroyWebView(tab: Tab) {
        val wv = tab.webView ?: return
        tab.webView = null
        tab.scriptHandler = null
        tab.scriptKey = null
        (wv.parent as? ViewGroup)?.removeView(wv)
        wv.stopLoading()
        wv.destroy()
    }

    /** The tab's renderer died: make a fresh WebView and reload the page. */
    fun recreateWebView(tab: Tab) {
        val wasCurrent = tab === current
        destroyWebView(tab)
        if (wasCurrent) {
            current = null
            selectTab(tab)
        }
    }

    // --- WebView ---

    @SuppressLint("SetJavaScriptEnabled")
    fun ensureWebView(tab: Tab): WebView {
        tab.webView?.let { return it }
        val wv = WebView(this)
        tab.webView = wv
        wv.settings.apply {
            javaScriptEnabled = true
            domStorageEnabled = true
            // Local files and content providers are never readable by pages.
            allowFileAccess = false
            allowContentAccess = false
            mixedContentMode = WebSettings.MIXED_CONTENT_NEVER_ALLOW
            setSupportMultipleWindows(true)
            javaScriptCanOpenWindowsAutomatically = false
            setSupportZoom(true)
            builtInZoomControls = true
            displayZoomControls = false
            useWideViewPort = true
            loadWithOverviewMode = true
            setGeolocationEnabled(true)
        }
        @Suppress("DEPRECATION")
        wv.settings.apply {
            // Explicit even though they are the defaults: pages never get file access.
            allowFileAccessFromFileURLs = false
            allowUniversalAccessFromFileURLs = false
            databaseEnabled = true
        }
        if (WebViewFeature.isFeatureSupported(WebViewFeature.SAFE_BROWSING_ENABLE)) WebSettingsCompat.setSafeBrowsingEnabled(wv.settings, true)
        if (WebViewFeature.isFeatureSupported(WebViewFeature.ALGORITHMIC_DARKENING)) {
            WebSettingsCompat.setAlgorithmicDarkeningAllowed(wv.settings, true)
        } else if (WebViewFeature.isFeatureSupported(WebViewFeature.FORCE_DARK) && isNight()) {
            @Suppress("DEPRECATION") WebSettingsCompat.setForceDark(wv.settings, WebSettingsCompat.FORCE_DARK_ON)
        }
        WebView.setWebContentsDebuggingEnabled(BuildConfig.DEBUG)
        applyUserAgent(tab, wv)
        wv.webViewClient = TabClient(this, tab)
        wv.webChromeClient = TabChromeClient(this, tab)
        wv.setDownloadListener { url, ua, cd, mime, _ -> download(url, ua, cd, mime) }
        wv.setFindListener { active, count, _ ->
            findCount.text = if (findInput.text.isEmpty()) "" else "${if (count == 0) 0 else active + 1}/$count"
        }
        wv.setOnLongClickListener { onLongPress(wv) }
        if (tab.url.isNotEmpty() && Urls.isAllowedTopLevel(tab.url)) {
            tab.topUrl = tab.url
            DocumentStart.prepare(tab, tab.url, settings)
            wv.loadUrl(tab.url)
        }
        return wv
    }

    private fun isNight() = (resources.configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK) == Configuration.UI_MODE_NIGHT_YES

    private fun applyUserAgent(tab: Tab, wv: WebView) {
        val ua = WebSettings.getDefaultUserAgent(this)
        wv.settings.userAgentString = if (tab.desktop) {
            ua.replace(Regex("\\(Linux; Android[^)]*\\)"), "(X11; Linux x86_64)").replace(" Mobile", "")
        } else {
            ua
        }
    }

    private fun onLongPress(wv: WebView): Boolean {
        val r = wv.hitTestResult
        val url = when (r.type) {
            WebView.HitTestResult.SRC_ANCHOR_TYPE, WebView.HitTestResult.SRC_IMAGE_ANCHOR_TYPE -> r.extra
            else -> null
        } ?: return false
        if (!Urls.isWeb(url)) return false
        AlertDialog.Builder(this).setTitle(url.take(100))
            .setItems(arrayOf("Open in new tab", "Copy link", "Share link")) { _, which ->
                when (which) {
                    0 -> newTab(url, select = false, opener = current?.id ?: -1).also { load(it, url, show = false) }
                    1 -> (getSystemService(CLIPBOARD_SERVICE) as android.content.ClipboardManager)
                        .setPrimaryClip(android.content.ClipData.newPlainText("link", url))
                    2 -> share(url)
                }
            }.show()
        return true
    }

    // --- navigation ---

    /** Address bar or another UI asks to open [input] (URL or search words) in the current tab. */
    fun navigate(input: String) {
        val url = Urls.resolveInput(input, settings.searchEngine)
        if (!Urls.isAllowedTopLevel(url)) return
        val tab = current ?: newTab()
        load(tab, url)
    }

    /** Open [url] (e.g. an intent: fallback) in a new tab. */
    fun openUrl(url: String) {
        if (Urls.isWeb(url)) newTab(url, select = true).also { load(it, url) }
    }

    fun load(tab: Tab, url: String, show: Boolean = true) {
        tab.url = if (url == "about:blank") "" else url
        tab.hasPage = true
        tab.topUrl = url
        tab.blocked.set(0)
        val wasCurrent = tab === current
        val wv = ensureWebView(tab)
        if (wasCurrent && show && wv.parent == null) {
            content.removeAllViews()
            content.addView(wv, ViewGroup.LayoutParams(-1, -1))
            wv.onResume()
        }
        // Register the document-start script for the target before navigating.
        DocumentStart.prepare(tab, url, settings)
        if (tab.url.isEmpty() && url != "about:blank") tab.url = url
        wv.loadUrl(url)
        if (wasCurrent) hideKeyboard()
        updateUi()
    }

    private fun goBack() {
        val tab = current
        when {
            customView != null -> hideCustomView()
            findBar.visibility == View.VISIBLE -> closeFind()
            tab?.webView?.canGoBack() == true -> tab.webView?.goBack()
            tab != null && tab.openerId >= 0 && tabs.any { it.id == tab.openerId } -> closeTab(tab)
            tab != null && tab.hasPage && tabs.size > 1 -> closeTab(tab)
            else -> moveTaskToBack(true)
        }
    }

    // --- UI wiring ---

    private fun setupAddressBar() {
        address.setAdapter(SuggestAdapter(this, db))
        address.setOnItemClickListener { parent, _, pos, _ ->
            (parent.getItemAtPosition(pos) as? Entry)?.let { navigate(it.url) }
        }
        address.setOnEditorActionListener { v, actionId, event ->
            if (actionId == EditorInfo.IME_ACTION_GO || (event?.keyCode == KeyEvent.KEYCODE_ENTER && event.action == KeyEvent.ACTION_DOWN)) {
                address.dismissDropDown()
                navigate(v.text.toString())
                true
            } else {
                false
            }
        }
        address.setOnFocusChangeListener { _, focus -> if (!focus) updateAddressText() }
    }

    private fun setupButtons() {
        backButton.setOnClickListener { current?.webView?.takeIf { it.canGoBack() }?.goBack() }
        forwardButton.setOnClickListener { current?.webView?.takeIf { it.canGoForward() }?.goForward() }
        reloadButton.setOnClickListener {
            val t = current ?: return@setOnClickListener
            if (t.loading) t.webView?.stopLoading() else if (t.isBlank) Unit else t.webView?.reload()
        }
        findViewById<View>(R.id.newtab).setOnClickListener {
            val hp = settings.homepage
            newTab(hp, select = true).also { if (hp.isNotEmpty()) load(it, hp) else address.requestFocus() }
        }
        tabsButton.setOnClickListener { TabSwitcher.show(this) }
        findViewById<View>(R.id.menu).setOnClickListener { showMenu(it) }
        shield.setOnClickListener { showShieldMenu(it) }

        findViewById<View>(R.id.find_close).setOnClickListener { closeFind() }
        findViewById<View>(R.id.find_next).setOnClickListener { current?.webView?.findNext(true) }
        findViewById<View>(R.id.find_prev).setOnClickListener { current?.webView?.findNext(false) }
        findInput.addTextChangedListener(object : android.text.TextWatcher {
            override fun afterTextChanged(s: android.text.Editable) {
                val wv = current?.webView ?: return
                if (s.isEmpty()) wv.clearMatches() else wv.findAllAsync(s.toString())
            }
            override fun beforeTextChanged(s: CharSequence?, a: Int, b: Int, c: Int) = Unit
            override fun onTextChanged(s: CharSequence?, a: Int, b: Int, c: Int) = Unit
        })
        findInput.setOnEditorActionListener { _, _, _ ->
            current?.webView?.findNext(true)
            true
        }
    }

    private fun showMenu(anchor: View) {
        val tab = current
        val pm = PopupMenu(this, anchor)
        val m = pm.menu
        val web = tab != null && Urls.isWeb(tab.url)
        val update = settings.update
        if (update != null) m.add(0, 100, 0, "Update available: ${update.first}")
        m.add(0, 1, 1, "Share").isEnabled = web
        m.add(0, 2, 2, "Find in page").isEnabled = web
        m.add(0, 3, 3, "Desktop site").apply { isCheckable = true; isChecked = tab?.desktop == true }
        m.add(0, 4, 4, if (tab != null && db.isBookmarked(tab.url)) "Remove bookmark" else "Add bookmark").isEnabled = web
        m.add(0, 5, 5, "Bookmarks")
        m.add(0, 6, 6, "History")
        m.add(0, 7, 7, "Downloads")
        m.add(0, 8, 8, "Settings")
        pm.setOnMenuItemClickListener { item ->
            when (item.itemId) {
                100 -> update?.let { openUrl(it.second) }
                1 -> tab?.let { share(it.url) }
                2 -> openFind()
                3 -> tab?.let { toggleDesktop(it) }
                4 -> tab?.let {
                    val added = db.toggleBookmark(it.url, it.title)
                    toast(if (added) "Bookmark added" else "Bookmark removed")
                }
                5 -> libraryLauncher.launch(Intent(this, LibraryActivity::class.java).putExtra(LibraryActivity.MODE, LibraryActivity.BOOKMARKS))
                6 -> libraryLauncher.launch(Intent(this, LibraryActivity::class.java).putExtra(LibraryActivity.MODE, LibraryActivity.HISTORY))
                7 -> startActivity(Intent(DownloadManager.ACTION_VIEW_DOWNLOADS))
                8 -> startActivity(Intent(this, SettingsActivity::class.java))
            }
            true
        }
        pm.show()
    }

    private fun showShieldMenu(anchor: View) {
        val tab = current ?: return
        val pm = PopupMenu(this, anchor)
        val m = pm.menu
        val site = Urls.siteKey(tab.url)
        when {
            !AdBlocker.available -> m.add("Ad blocking is unavailable in this build").isEnabled = false
            !settings.adblock -> m.add("Ad blocking is off in Settings").isEnabled = false
            else -> {
                m.add("Blocked on this page: ${tab.blocked.get()}").isEnabled = false
                if (!AdBlocker.ready) m.add("Filter lists are still loading").isEnabled = false
                if (site != null) {
                    val excepted = Urls.isException(settings.exceptions, tab.url)
                    m.add(0, 1, 0, if (excepted) "Turn on blocking on this site" else "Turn off blocking on this site")
                }
            }
        }
        m.add(0, 2, 9, "Ad blocking settings")
        pm.setOnMenuItemClickListener { item ->
            when (item.itemId) {
                1 -> {
                    if (Urls.isException(settings.exceptions, tab.url)) {
                        // The exception may be a parent domain of this host: drop every matching one.
                        settings.exceptions = settings.exceptions.filterNot { Urls.isException(listOf(it), tab.url) }.toSet()
                    } else {
                        site?.let { settings.addException(it) }
                    }
                    tab.scriptKey = null
                    tab.webView?.reload()
                }
                2 -> startActivity(Intent(this, SettingsActivity::class.java))
            }
            true
        }
        pm.show()
    }

    private fun toggleDesktop(tab: Tab) {
        tab.desktop = !tab.desktop
        tab.webView?.let {
            applyUserAgent(tab, it)
            it.reload()
        }
        saveSession()
    }

    private fun share(url: String) {
        startActivity(Intent.createChooser(Intent(Intent.ACTION_SEND).setType("text/plain").putExtra(Intent.EXTRA_TEXT, url), "Share link"))
    }

    private fun openFind() {
        findBar.visibility = View.VISIBLE
        findInput.requestFocus()
        (getSystemService(INPUT_METHOD_SERVICE) as InputMethodManager).showSoftInput(findInput, 0)
    }

    private fun closeFind() {
        current?.webView?.clearMatches()
        findInput.setText("")
        findBar.visibility = View.GONE
        hideKeyboard()
    }

    private fun hideKeyboard() {
        address.clearFocus()
        (getSystemService(INPUT_METHOD_SERVICE) as InputMethodManager).hideSoftInputFromWindow(main.windowToken, 0)
    }

    private fun toast(s: String) = Toast.makeText(this, s, Toast.LENGTH_SHORT).show()

    // --- UI state (UI thread) ---

    fun tabChanged(tab: Tab) {
        if (tab === current) updateUi()
    }

    fun blockedChanged(tab: Tab) {
        if (tab === current) ui.post { updateShield() }
    }

    private fun updateAddressText() {
        val tab = current
        if (!address.hasFocus()) address.setText(if (tab == null || tab.isBlank || !Urls.isWeb(tab.url)) "" else tab.url, false)
    }

    private fun updateShield() {
        val tab = current
        val active = tab != null && AdBlocker.available && settings.blockingFor(tab.url)
        shield.text = if (active) tab!!.blocked.get().toString() else ""
        shield.alpha = if (active) 1f else 0.4f
    }

    fun updateUi() {
        val tab = current ?: return
        updateAddressText()
        updateShield()
        tabsButton.text = tabs.size.toString()
        val wv = tab.webView
        backButton.alpha = if (wv?.canGoBack() == true) 1f else 0.4f
        forwardButton.alpha = if (wv?.canGoForward() == true) 1f else 0.4f
        reloadButton.setImageResource(if (tab.loading) R.drawable.ic_close else R.drawable.ic_refresh)
        progress.visibility = if (tab.loading && tab.progress < 100) View.VISIBLE else View.INVISIBLE
        progress.progress = tab.progress
        if (!tab.loading) refresh.isRefreshing = false
        if (tab.isBlank && newTabView.view.parent == null) {
            content.removeAllViews()
            newTabView.refresh(db)
            content.addView(newTabView.view)
        } else if (!tab.isBlank && newTabView.view.parent != null) {
            content.removeAllViews()
            tab.webView?.let { content.addView(it, ViewGroup.LayoutParams(-1, -1)) }
        }
    }

    // --- downloads ---

    private fun download(url: String, ua: String?, contentDisposition: String?, mime: String?) {
        if (!Urls.isWeb(url)) {
            toast("This kind of download is not supported.")
            return
        }
        val start = {
            val name = URLUtil.guessFileName(url, contentDisposition, mime)
            try {
                val req = DownloadManager.Request(Uri.parse(url))
                    .setNotificationVisibility(DownloadManager.Request.VISIBILITY_VISIBLE_NOTIFY_COMPLETED)
                    .setDestinationInExternalPublicDir(Environment.DIRECTORY_DOWNLOADS, name)
                    .setTitle(name)
                if (!mime.isNullOrEmpty()) req.setMimeType(mime)
                if (!ua.isNullOrEmpty()) req.addRequestHeader("User-Agent", ua)
                CookieManager.getInstance().getCookie(url)?.let { req.addRequestHeader("Cookie", it) }
                (getSystemService(DOWNLOAD_SERVICE) as DownloadManager).enqueue(req)
                toast("Downloading $name")
            } catch (e: Exception) {
                toast("Download failed: ${e.message}")
            }
        }
        if (Build.VERSION.SDK_INT <= Build.VERSION_CODES.P &&
            ContextCompat.checkSelfPermission(this, Manifest.permission.WRITE_EXTERNAL_STORAGE) != PackageManager.PERMISSION_GRANTED
        ) {
            pendingDownload = start
            storageLauncher.launch(Manifest.permission.WRITE_EXTERNAL_STORAGE)
        } else {
            start()
        }
    }

    // --- file chooser, fullscreen video ---

    fun showFileChooser(cb: ValueCallback<Array<Uri>>, params: WebChromeClient.FileChooserParams): Boolean {
        fileCallback?.onReceiveValue(null)
        fileCallback = cb
        return try {
            fileLauncher.launch(params.createIntent())
            true
        } catch (_: ActivityNotFoundException) {
            fileCallback = null
            cb.onReceiveValue(null)
            false
        }
    }

    fun showCustomView(view: View, cb: WebChromeClient.CustomViewCallback) {
        if (customView != null) {
            cb.onCustomViewHidden()
            return
        }
        customView = view
        customCallback = cb
        main.visibility = View.GONE
        root.addView(view, FrameLayout.LayoutParams(-1, -1))
        window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        WindowInsetsControllerCompat(window, root).apply {
            systemBarsBehavior = WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
            hide(WindowInsetsCompat.Type.systemBars())
        }
    }

    fun hideCustomView() {
        val v = customView ?: return
        root.removeView(v)
        customView = null
        customCallback?.onCustomViewHidden()
        customCallback = null
        main.visibility = View.VISIBLE
        window.clearFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        WindowCompat.getInsetsController(window, root).show(WindowInsetsCompat.Type.systemBars())
    }

    // --- site permissions (camera, microphone, location) ---

    private fun ask(origin: String, key: String, label: String, decided: (Boolean) -> Unit) {
        db.permission(origin, key)?.let { decided(it); return }
        AlertDialog.Builder(this)
            .setTitle("Allow $label?")
            .setMessage("$origin wants to use your $label. Your choice is remembered for this site.")
            .setPositiveButton("Allow") { _, _ -> db.setPermission(origin, key, true); decided(true) }
            .setNegativeButton("Block") { _, _ -> db.setPermission(origin, key, false); decided(false) }
            .setOnCancelListener { decided(false) }
            .show()
    }

    private fun androidPermissions(perms: List<String>, done: (Boolean) -> Unit) {
        val missing = perms.filter { ContextCompat.checkSelfPermission(this, it) != PackageManager.PERMISSION_GRANTED }
        if (missing.isEmpty()) {
            done(true)
            return
        }
        permissionCallback?.invoke(emptyMap())
        permissionCallback = { r -> done(missing.all { r[it] == true }) }
        permissionLauncher.launch(missing.toTypedArray())
    }

    fun onWebPermissionRequest(request: PermissionRequest) {
        val origin = request.origin.toString()
        val wanted = request.resources.filter {
            it == PermissionRequest.RESOURCE_VIDEO_CAPTURE || it == PermissionRequest.RESOURCE_AUDIO_CAPTURE
        }
        if (wanted.isEmpty()) {
            request.deny()
            return
        }
        val granted = mutableListOf<String>()
        fun next(i: Int) {
            if (i == wanted.size) {
                if (granted.isEmpty()) {
                    request.deny()
                    return
                }
                val android = granted.map {
                    if (it == PermissionRequest.RESOURCE_VIDEO_CAPTURE) Manifest.permission.CAMERA else Manifest.permission.RECORD_AUDIO
                }
                androidPermissions(android) { ok -> if (ok) request.grant(granted.toTypedArray()) else request.deny() }
                return
            }
            val res = wanted[i]
            val video = res == PermissionRequest.RESOURCE_VIDEO_CAPTURE
            ask(origin, if (video) "camera" else "microphone", if (video) "camera" else "microphone") { ok ->
                if (ok) granted.add(res)
                next(i + 1)
            }
        }
        next(0)
    }

    fun onGeolocationRequest(origin: String, callback: GeolocationPermissions.Callback) {
        ask(origin, "location", "location") { ok ->
            if (!ok) {
                callback.invoke(origin, false, false)
            } else {
                androidPermissions(listOf(Manifest.permission.ACCESS_FINE_LOCATION)) { granted -> callback.invoke(origin, granted, false) }
            }
        }
    }
}
