package se.ferrite.browser

import android.content.Context
import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import android.widget.BaseAdapter
import android.widget.Filter
import android.widget.Filterable
import android.widget.ImageView
import android.widget.TextView

/** Address-bar suggestions from bookmarks and history. */
class SuggestAdapter(private val ctx: Context, private val db: Db) : BaseAdapter(), Filterable {
    private var items: List<Entry> = emptyList()

    override fun getCount() = items.size
    override fun getItem(position: Int) = items[position]
    override fun getItemId(position: Int) = items[position].id

    override fun getView(position: Int, convertView: View?, parent: ViewGroup): View {
        val v = convertView ?: LayoutInflater.from(ctx).inflate(R.layout.row_entry, parent, false)
        val e = items[position]
        v.findViewById<TextView>(R.id.title).text = e.title.ifEmpty { e.url }
        v.findViewById<TextView>(R.id.subtitle).text = e.url
        v.findViewById<ImageView>(R.id.icon).visibility = View.GONE
        return v
    }

    override fun getFilter() = object : Filter() {
        override fun performFiltering(constraint: CharSequence?): FilterResults {
            val list = db.suggest(constraint?.toString().orEmpty())
            return FilterResults().apply { values = list; count = list.size }
        }

        @Suppress("UNCHECKED_CAST")
        override fun publishResults(constraint: CharSequence?, results: FilterResults) {
            items = results.values as? List<Entry> ?: emptyList()
            notifyDataSetChanged()
        }

        // The text field would show Entry.toString() after a pick; navigation replaces it.
        override fun convertResultToString(resultValue: Any?) = (resultValue as? Entry)?.url ?: ""
    }
}
