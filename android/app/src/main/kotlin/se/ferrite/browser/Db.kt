package se.ferrite.browser

import android.content.ContentValues
import android.content.Context
import android.database.sqlite.SQLiteDatabase
import android.database.sqlite.SQLiteOpenHelper
import java.util.concurrent.Executors

data class Entry(val id: Long, val url: String, val title: String, val time: Long)

/** History, bookmarks and per-site permission decisions in one SQLite file. Writes run on a background thread. */
class Db(ctx: Context) : SQLiteOpenHelper(ctx.applicationContext, "ferrite.db", null, 1) {
    private val writer = Executors.newSingleThreadExecutor { r -> Thread(r, "db-writer").apply { isDaemon = true } }

    override fun onCreate(db: SQLiteDatabase) {
        db.execSQL("CREATE TABLE history (id INTEGER PRIMARY KEY, url TEXT NOT NULL UNIQUE, title TEXT NOT NULL DEFAULT '', last_visit INTEGER NOT NULL, visits INTEGER NOT NULL DEFAULT 1)")
        db.execSQL("CREATE TABLE bookmarks (id INTEGER PRIMARY KEY, url TEXT NOT NULL UNIQUE, title TEXT NOT NULL DEFAULT '', added INTEGER NOT NULL)")
        db.execSQL("CREATE TABLE permissions (origin TEXT NOT NULL, perm TEXT NOT NULL, allow INTEGER NOT NULL, PRIMARY KEY (origin, perm))")
    }

    override fun onUpgrade(db: SQLiteDatabase, oldVersion: Int, newVersion: Int) = Unit

    private fun async(block: SQLiteDatabase.() -> Unit) {
        writer.execute { runCatching { writableDatabase.block() } }
    }

    // --- history ---

    fun addVisit(url: String, title: String) {
        if (!Urls.isWeb(url)) return
        val now = System.currentTimeMillis()
        async {
            execSQL(
                "INSERT INTO history (url, title, last_visit) VALUES (?, ?, ?) " +
                    "ON CONFLICT(url) DO UPDATE SET last_visit = excluded.last_visit, visits = visits + 1, " +
                    "title = CASE WHEN excluded.title != '' THEN excluded.title ELSE title END",
                arrayOf<Any?>(url, title, now),
            )
        }
    }

    fun updateTitle(url: String, title: String) {
        if (title.isEmpty() || !Urls.isWeb(url)) return
        async { execSQL("UPDATE history SET title = ? WHERE url = ?", arrayOf(title, url)) }
    }

    fun history(query: String, limit: Int = 300): List<Entry> = list("history", "last_visit", query, limit)

    fun deleteHistory(id: Long) = async { delete("history", "id = ?", arrayOf(id.toString())) }

    fun clearHistory() = async { delete("history", null, null) }

    /** Most visited pages, for the new-tab page. */
    fun topSites(limit: Int): List<Entry> = readableDatabase.rawQuery(
        "SELECT id, url, title, last_visit FROM history ORDER BY visits DESC, last_visit DESC LIMIT ?",
        arrayOf(limit.toString()),
    ).use { c -> generateSequence { if (c.moveToNext()) Entry(c.getLong(0), c.getString(1), c.getString(2), c.getLong(3)) else null }.toList() }

    // --- bookmarks ---

    fun bookmarks(query: String): List<Entry> = list("bookmarks", "added", query, 500)

    fun isBookmarked(url: String): Boolean = readableDatabase.rawQuery("SELECT 1 FROM bookmarks WHERE url = ?", arrayOf(url)).use { it.moveToFirst() }

    /** Adds the bookmark, or removes it if it exists. Returns true if it is now bookmarked. */
    fun toggleBookmark(url: String, title: String): Boolean {
        if (!Urls.isWeb(url)) return false
        val had = isBookmarked(url)
        async {
            if (had) delete("bookmarks", "url = ?", arrayOf(url))
            else insertWithOnConflict("bookmarks", null, ContentValues().apply {
                put("url", url)
                put("title", title)
                put("added", System.currentTimeMillis())
            }, SQLiteDatabase.CONFLICT_IGNORE)
        }
        return !had
    }

    fun deleteBookmark(id: Long) = async { delete("bookmarks", "id = ?", arrayOf(id.toString())) }

    private fun list(table: String, order: String, query: String, limit: Int): List<Entry> {
        val like = "%" + query.replace("\\", "\\\\").replace("%", "\\%").replace("_", "\\_") + "%"
        val col = if (table == "history") "last_visit" else "added"
        return readableDatabase.rawQuery(
            "SELECT id, url, title, $col FROM $table WHERE url LIKE ? ESCAPE '\\' OR title LIKE ? ESCAPE '\\' ORDER BY $order DESC LIMIT ?",
            arrayOf(like, like, limit.toString()),
        ).use { c -> generateSequence { if (c.moveToNext()) Entry(c.getLong(0), c.getString(1), c.getString(2), c.getLong(3)) else null }.toList() }
    }

    /** Address-bar suggestions: bookmarks first, then history. */
    fun suggest(query: String, limit: Int = 8): List<Entry> {
        if (query.isBlank()) return emptyList()
        val seen = HashSet<String>()
        return (bookmarks(query).take(limit) + history(query, limit * 2)).filter { seen.add(it.url) }.take(limit)
    }

    // --- site permissions ---

    /** null = not decided yet (ask). */
    fun permission(origin: String, perm: String): Boolean? = readableDatabase.rawQuery(
        "SELECT allow FROM permissions WHERE origin = ? AND perm = ?", arrayOf(origin, perm),
    ).use { if (it.moveToFirst()) it.getInt(0) != 0 else null }

    fun setPermission(origin: String, perm: String, allow: Boolean) = async {
        execSQL("INSERT OR REPLACE INTO permissions (origin, perm, allow) VALUES (?, ?, ?)", arrayOf<Any?>(origin, perm, if (allow) 1 else 0))
    }

    fun clearPermissions() = async { delete("permissions", null, null) }
}
