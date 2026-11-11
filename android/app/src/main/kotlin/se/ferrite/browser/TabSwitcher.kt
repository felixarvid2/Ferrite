package se.ferrite.browser

import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import android.widget.ImageView
import android.widget.TextView
import androidx.recyclerview.widget.LinearLayoutManager
import androidx.recyclerview.widget.RecyclerView
import com.google.android.material.bottomsheet.BottomSheetDialog

/** Bottom sheet listing the open tabs with title and favicon; tap to switch, X to close. */
object TabSwitcher {
    fun show(a: BrowserActivity) {
        val dialog = BottomSheetDialog(a)
        val view = LayoutInflater.from(a).inflate(R.layout.sheet_tabs, null)
        val list = view.findViewById<RecyclerView>(R.id.tab_list)
        val adapter = Adapter(a, dialog)
        list.layoutManager = LinearLayoutManager(a)
        list.adapter = adapter
        view.findViewById<View>(R.id.sheet_new).setOnClickListener {
            dialog.dismiss()
            val hp = a.settings.homepage
            a.newTab(hp, select = true).also { if (hp.isNotEmpty()) a.load(it, hp) }
        }
        dialog.setContentView(view)
        dialog.show()
    }

    private class Adapter(val a: BrowserActivity, val dialog: BottomSheetDialog) : RecyclerView.Adapter<Holder>() {
        override fun getItemCount() = a.tabs.size

        override fun onCreateViewHolder(parent: ViewGroup, viewType: Int) =
            Holder(LayoutInflater.from(parent.context).inflate(R.layout.row_entry, parent, false))

        override fun onBindViewHolder(h: Holder, position: Int) {
            val tab = a.tabs[position]
            h.title.text = tab.displayTitle
            h.subtitle.text = if (tab.isBlank) "" else tab.url
            h.icon.setImageBitmap(tab.favicon)
            if (tab.favicon == null) h.icon.setImageResource(R.drawable.ic_shield)
            h.itemView.alpha = if (tab === a.current) 1f else 0.75f
            h.itemView.setOnClickListener {
                dialog.dismiss()
                a.selectTab(tab)
            }
            h.action.visibility = View.VISIBLE
            h.action.setOnClickListener {
                val i = h.bindingAdapterPosition
                if (i < 0) return@setOnClickListener
                a.closeTab(tab)
                if (a.tabs.size == 1 && a.tabs[0].isBlank && tab.isBlank) dialog.dismiss()
                notifyDataSetChanged()
            }
        }
    }

    private class Holder(v: View) : RecyclerView.ViewHolder(v) {
        val icon: ImageView = v.findViewById(R.id.icon)
        val title: TextView = v.findViewById(R.id.title)
        val subtitle: TextView = v.findViewById(R.id.subtitle)
        val action: View = v.findViewById(R.id.action)
    }
}
