package se.ferrite.browser

import android.content.Context
import android.graphics.Typeface
import android.util.TypedValue
import android.view.Gravity
import android.view.View
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView

/** The native new-tab page: a grid of top sites (most visited) and bookmarks. */
class NewTabView(private val ctx: Context, private val open: (String) -> Unit) {
    private val column = LinearLayout(ctx).apply {
        orientation = LinearLayout.VERTICAL
        gravity = Gravity.CENTER_HORIZONTAL
        setPadding(dp(16), dp(48), dp(16), dp(24))
    }
    val view: View = ScrollView(ctx).apply { addView(column) }

    private fun dp(v: Int) = (v * ctx.resources.displayMetrics.density).toInt()

    fun refresh(db: Db) {
        column.removeAllViews()
        column.addView(TextView(ctx).apply {
            text = "Ferrite"
            setTextSize(TypedValue.COMPLEX_UNIT_SP, 32f)
            typeface = Typeface.DEFAULT_BOLD
            gravity = Gravity.CENTER
            setPadding(0, 0, 0, dp(32))
        })
        val sites = (db.bookmarks("").take(8) + db.topSites(12)).distinctBy { it.url }.take(12)
        if (sites.isEmpty()) {
            column.addView(TextView(ctx).apply {
                text = "Type an address or a search in the bar above. The sites you visit most appear here."
                gravity = Gravity.CENTER
            })
            return
        }
        for (rowSites in sites.chunked(4)) {
            val row = LinearLayout(ctx).apply { orientation = LinearLayout.HORIZONTAL; weightSum = 4f }
            for (s in rowSites) row.addView(tile(s))
            column.addView(row, LinearLayout.LayoutParams(-1, -2))
        }
    }

    private fun tile(e: Entry): View {
        val host = Urls.siteKey(e.url) ?: e.url
        val label = e.title.ifEmpty { host }
        return LinearLayout(ctx).apply {
            orientation = LinearLayout.VERTICAL
            gravity = Gravity.CENTER_HORIZONTAL
            setPadding(dp(4), dp(8), dp(4), dp(8))
            layoutParams = LinearLayout.LayoutParams(0, -2, 1f)
            val circle = TextView(ctx).apply {
                text = host.take(1).uppercase()
                gravity = Gravity.CENTER
                setTextSize(TypedValue.COMPLEX_UNIT_SP, 22f)
                setBackgroundResource(R.drawable.bg_tile)
            }
            addView(circle, LinearLayout.LayoutParams(dp(56), dp(56)))
            addView(TextView(ctx).apply {
                text = label
                maxLines = 1
                ellipsize = android.text.TextUtils.TruncateAt.END
                gravity = Gravity.CENTER
                setTextSize(TypedValue.COMPLEX_UNIT_SP, 12f)
                setPadding(0, dp(6), 0, 0)
            })
            isClickable = true
            contentDescription = label
            setOnClickListener { open(e.url) }
        }
    }
}
