package se.ferrite.browser

import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.text.method.LinkMovementMethod
import android.view.Gravity
import android.widget.Button
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView
import androidx.activity.enableEdgeToEdge
import androidx.appcompat.app.AlertDialog
import androidx.appcompat.app.AppCompatActivity
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import se.ferrite.browser.adblock.AdBlocker

class AboutActivity : AppCompatActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        title = "About Ferrite"
        val pad = (16 * resources.displayMetrics.density).toInt()
        val col = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL; setPadding(pad, pad, pad, pad) }
        val scroll = ScrollView(this).apply { addView(col) }
        setContentView(scroll)
        ViewCompat.setOnApplyWindowInsetsListener(scroll) { v, insets ->
            val b = insets.getInsets(WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout())
            v.setPadding(b.left, b.top, b.right, b.bottom)
            WindowInsetsCompat.CONSUMED
        }

        fun text(s: CharSequence, size: Float = 14f, bold: Boolean = false) = TextView(this).apply {
            text = s
            textSize = size
            if (bold) setTypeface(typeface, android.graphics.Typeface.BOLD)
            setPadding(0, pad / 2, 0, pad / 2)
            movementMethod = LinkMovementMethod.getInstance()
            setTextIsSelectable(true)
        }
        col.addView(text("Ferrite for Android", 24f, true))
        col.addView(text("Version ${BuildConfig.VERSION_NAME}"))
        col.addView(text("Web engine: Android System WebView (Chromium) ${webViewVersion()}"))
        col.addView(text("Ad blocking engine: ${if (AdBlocker.available) "loaded" else "not part of this build"}${if (AdBlocker.ready) ", filter lists active" else ""}"))
        val status = text(updateText())
        col.addView(status)
        col.addView(Button(this).apply {
            text = "Check for updates now"
            setOnClickListener {
                app.checkForUpdate(force = true) { Handler(Looper.getMainLooper()).post { status.text = updateText() } }
                status.text = "Checking..."
            }
        }, LinearLayout.LayoutParams(-2, -2).apply { gravity = Gravity.START })

        col.addView(text("Licences", 18f, true))
        col.addView(text("Ferrite is released under the MIT licence."))
        col.addView(text("Ad blocking uses adblock-rust by Brave Software (MPL-2.0, https://github.com/brave/adblock-rust)."))
        col.addView(text("Scriptlets and filter lists come from uBlock Origin (GPL-3.0, Copyright (C) Raymond Hill and contributors, https://github.com/gorhill/uBlock) and from EasyList and EasyPrivacy (https://easylist.to). The scriptlets ship as a separate data file in the app."))
        col.addView(Button(this).apply {
            text = "uBlock Origin licence text"
            setOnClickListener { showAsset("LICENSE-uBlock-Origin.txt", "uBlock Origin (GPL-3.0)") }
        }, LinearLayout.LayoutParams(-2, -2))
        col.addView(text("Web pages are rendered by Android System WebView, which is built from Chromium (BSD-3-Clause and other licences, https://chromium.googlesource.com/chromium/src/+/main/LICENSE). AndroidX, Material Components and the Rust crates used are under the Apache-2.0 or MIT licences."))
        col.addView(text("Ferrite for Android has no extensions and no password manager integration."))
    }

    private fun updateText(): String {
        val u = app.settings.update
        return if (u != null) "Update available: ${u.first} (download from the menu of the browser)" else "No update known. Checked ${if (app.settings.lastUpdateCheck == 0L) "never" else "automatically once a day"}."
    }

    private fun webViewVersion(): String =
        androidx.webkit.WebViewCompat.getCurrentWebViewPackage(this)?.versionName ?: "unknown"

    private fun showAsset(name: String, title: String) {
        val text = try {
            assets.open(name).bufferedReader().use { it.readText() }
        } catch (_: Exception) {
            "Licence text not found."
        }
        val tv = TextView(this).apply {
            this.text = text
            textSize = 11f
            setTextIsSelectable(true)
            setPadding(32, 16, 32, 16)
        }
        AlertDialog.Builder(this).setTitle(title).setView(ScrollView(this).apply { addView(tv) })
            .setPositiveButton("Close", null).show()
    }
}
