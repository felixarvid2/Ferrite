package se.ferrite.browser

import android.content.Intent
import android.os.Bundle
import android.text.Editable
import android.text.TextWatcher
import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import android.widget.EditText
import android.widget.ImageView
import android.widget.TextView
import androidx.activity.enableEdgeToEdge
import androidx.appcompat.app.AlertDialog
import androidx.appcompat.app.AppCompatActivity
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import androidx.recyclerview.widget.LinearLayoutManager
import androidx.recyclerview.widget.RecyclerView
import com.google.android.material.appbar.MaterialToolbar
import java.text.DateFormat
import java.util.Date

/** Bookmarks and history lists with search; a tap returns the URL to the browser, a long press deletes. */
class LibraryActivity : AppCompatActivity() {
    private var mode = HISTORY
    private var entries: List<Entry> = emptyList()
    private lateinit var adapter: Adapter
    private lateinit var empty: TextView

    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_library)
        ViewCompat.setOnApplyWindowInsetsListener(findViewById(android.R.id.content)) { v, insets ->
            val b = insets.getInsets(WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout())
            v.setPadding(b.left, b.top, b.right, b.bottom)
            WindowInsetsCompat.CONSUMED
        }
        mode = intent.getStringExtra(MODE) ?: HISTORY
        val toolbar = findViewById<MaterialToolbar>(R.id.toolbar)
        toolbar.title = if (mode == BOOKMARKS) "Bookmarks" else "History"
        toolbar.setNavigationOnClickListener { finish() }
        if (mode == HISTORY) {
            toolbar.menu.add("Clear history").setShowAsAction(android.view.MenuItem.SHOW_AS_ACTION_NEVER)
            toolbar.setOnMenuItemClickListener {
                AlertDialog.Builder(this).setMessage("Clear all browsing history?")
                    .setPositiveButton("Clear") { _, _ -> app.db.clearHistory(); entries = emptyList(); show() }
                    .setNegativeButton("Cancel", null).show()
                true
            }
        }
        empty = findViewById(R.id.empty)
        empty.text = if (mode == BOOKMARKS) "No bookmarks yet. Use the menu on a page to add one." else "No history."
        adapter = Adapter()
        findViewById<RecyclerView>(R.id.list).apply {
            layoutManager = LinearLayoutManager(this@LibraryActivity)
            adapter = this@LibraryActivity.adapter
        }
        val search = findViewById<EditText>(R.id.search)
        search.addTextChangedListener(object : TextWatcher {
            override fun afterTextChanged(s: Editable) = load(s.toString())
            override fun beforeTextChanged(s: CharSequence?, a: Int, b: Int, c: Int) = Unit
            override fun onTextChanged(s: CharSequence?, a: Int, b: Int, c: Int) = Unit
        })
        load("")
    }

    private fun load(query: String) {
        entries = if (mode == BOOKMARKS) app.db.bookmarks(query) else app.db.history(query)
        show()
    }

    private fun show() {
        adapter.notifyDataSetChanged()
        empty.visibility = if (entries.isEmpty()) View.VISIBLE else View.GONE
    }

    private inner class Adapter : RecyclerView.Adapter<RecyclerView.ViewHolder>() {
        override fun getItemCount() = entries.size

        override fun onCreateViewHolder(parent: ViewGroup, viewType: Int) =
            object : RecyclerView.ViewHolder(LayoutInflater.from(parent.context).inflate(R.layout.row_entry, parent, false)) {}

        override fun onBindViewHolder(h: RecyclerView.ViewHolder, position: Int) {
            val e = entries[position]
            val v = h.itemView
            v.findViewById<ImageView>(R.id.icon).setImageResource(if (mode == BOOKMARKS) R.drawable.ic_shield else R.drawable.ic_refresh)
            v.findViewById<TextView>(R.id.title).text = e.title.ifEmpty { e.url }
            v.findViewById<TextView>(R.id.subtitle).text =
                if (mode == HISTORY) e.url + "  -  " + DateFormat.getDateTimeInstance(DateFormat.SHORT, DateFormat.SHORT).format(Date(e.time)) else e.url
            v.setOnClickListener {
                setResult(RESULT_OK, Intent().putExtra(RESULT_URL, e.url))
                finish()
            }
            v.setOnLongClickListener {
                AlertDialog.Builder(this@LibraryActivity).setMessage("Delete \"${e.title.ifEmpty { e.url }.take(60)}\"?")
                    .setPositiveButton("Delete") { _, _ ->
                        if (mode == BOOKMARKS) app.db.deleteBookmark(e.id) else app.db.deleteHistory(e.id)
                        entries = entries.filter { it.id != e.id }
                        show()
                    }.setNegativeButton("Cancel", null).show()
                true
            }
        }
    }

    companion object {
        const val MODE = "mode"
        const val BOOKMARKS = "bookmarks"
        const val HISTORY = "history"
        const val RESULT_URL = "url"
    }
}
