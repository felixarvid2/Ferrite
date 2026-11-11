package se.ferrite.browser

import android.content.Intent
import android.os.Bundle
import android.widget.EditText
import android.widget.Toast
import androidx.activity.enableEdgeToEdge
import androidx.appcompat.app.AlertDialog
import androidx.appcompat.app.AppCompatActivity
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import androidx.preference.Preference
import androidx.preference.PreferenceFragmentCompat
import se.ferrite.browser.adblock.AdBlocker
import java.text.DateFormat
import java.util.Date

class SettingsActivity : AppCompatActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        setContentView(android.widget.FrameLayout(this).apply { id = R.id.settings_container })
        ViewCompat.setOnApplyWindowInsetsListener(findViewById(R.id.settings_container)) { v, insets ->
            val b = insets.getInsets(WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout())
            v.setPadding(b.left, b.top, b.right, b.bottom)
            WindowInsetsCompat.CONSUMED
        }
        title = "Settings"
        if (savedInstanceState == null) {
            supportFragmentManager.beginTransaction().replace(R.id.settings_container, Fragment()).commit()
        }
    }

    class Fragment : PreferenceFragmentCompat() {
        override fun onCreatePreferences(savedInstanceState: Bundle?, rootKey: String?) {
            setPreferencesFromResource(R.xml.preferences, rootKey)
            val ctx = requireContext()
            val settings = ctx.app.settings

            findPreference<Preference>("exceptions")?.apply {
                fun summarize() {
                    val n = settings.exceptions.size
                    summary = if (n == 0) "None. Blocking applies on every site." else "$n sites"
                }
                summarize()
                setOnPreferenceClickListener {
                    showExceptions(settings) { summarize() }
                    true
                }
            }
            findPreference<Preference>("lists")?.apply {
                fun summarize() {
                    val (n, oldest) = AdBlocker.cacheInfo(ctx)
                    summary = when {
                        !AdBlocker.available -> "Ad blocking engine is not part of this build."
                        n == 0 -> "Not downloaded yet."
                        else -> "$n lists, updated ${DateFormat.getDateTimeInstance(DateFormat.SHORT, DateFormat.SHORT).format(Date(oldest))}. Refreshed daily."
                    }
                }
                summarize()
                setOnPreferenceClickListener {
                    ctx.app.refreshLists(force = true)
                    Toast.makeText(ctx, "Updating filter lists in the background", Toast.LENGTH_SHORT).show()
                    true
                }
            }
            findPreference<Preference>("permissions")?.setOnPreferenceClickListener {
                ctx.app.db.clearPermissions()
                Toast.makeText(ctx, "Site permissions reset", Toast.LENGTH_SHORT).show()
                true
            }
            findPreference<Preference>("clear")?.setOnPreferenceClickListener {
                val checked = booleanArrayOf(true, true, true, false)
                AlertDialog.Builder(ctx).setTitle("Clear browsing data")
                    .setMultiChoiceItems(BrowsingData.labels, checked) { _, i, on -> checked[i] = on }
                    .setPositiveButton("Clear") { _, _ ->
                        BrowsingData.clear(ctx, checked)
                        Toast.makeText(ctx, "Cleared", Toast.LENGTH_SHORT).show()
                    }
                    .setNegativeButton("Cancel", null).show()
                true
            }
            findPreference<Preference>("about")?.setOnPreferenceClickListener {
                startActivity(Intent(ctx, AboutActivity::class.java))
                true
            }
            findPreference<androidx.preference.EditTextPreference>("homepage")?.setOnBindEditTextListener {
                it.setSingleLine()
                it.inputType = android.text.InputType.TYPE_TEXT_VARIATION_URI
            }
        }

        private fun showExceptions(settings: Settings, changed: () -> Unit) {
            val ctx = requireContext()
            val sites = settings.exceptions.sorted().toTypedArray()
            val remove = BooleanArray(sites.size)
            val b = AlertDialog.Builder(ctx).setTitle("Sites without ad blocking")
            if (sites.isEmpty()) b.setMessage("No sites. Use the shield button on a page to turn blocking off for it.")
            else b.setMultiChoiceItems(sites, remove) { _, i, on -> remove[i] = on }
            b.setNeutralButton("Add site") { _, _ ->
                val input = EditText(ctx).apply { hint = "example.com"; setSingleLine() }
                AlertDialog.Builder(ctx).setTitle("Add site").setView(input)
                    .setPositiveButton("Add") { _, _ ->
                        val key = Urls.normalizeSite(input.text.toString())
                        if (key == null) Toast.makeText(ctx, "Not a valid site", Toast.LENGTH_SHORT).show() else settings.addException(key)
                        changed()
                    }.setNegativeButton("Cancel", null).show()
            }
            if (sites.isNotEmpty()) {
                b.setPositiveButton("Remove checked") { _, _ ->
                    settings.exceptions = settings.exceptions - sites.filterIndexed { i, _ -> remove[i] }.toSet()
                    changed()
                }
            }
            b.setNegativeButton("Close", null).show()
        }
    }
}
