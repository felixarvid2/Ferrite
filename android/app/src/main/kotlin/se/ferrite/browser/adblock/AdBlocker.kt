package se.ferrite.browser.adblock

import android.content.Context
import android.util.Log
import java.io.File
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.locks.ReentrantReadWriteLock
import kotlin.concurrent.read
import kotlin.concurrent.write

/** A filter list. Only uBO's own lists are trusted (may use trusted scriptlets). Same lists as the desktop `core/src/blocker.rs`. */
data class FilterList(val name: String, val url: String, val trusted: Boolean)

val FILTER_LISTS = listOf(
    FilterList("easylist.txt", "https://easylist.to/easylist/easylist.txt", false),
    FilterList("easyprivacy.txt", "https://easylist.to/easylist/easyprivacy.txt", false),
    FilterList("ubo-filters.txt", "https://raw.githubusercontent.com/uBlockOrigin/uAssets/master/filters/filters.txt", true),
    FilterList("ubo-quick-fixes.txt", "https://raw.githubusercontent.com/uBlockOrigin/uAssets/master/filters/quick-fixes.txt", true),
    FilterList("ubo-unbreak.txt", "https://raw.githubusercontent.com/uBlockOrigin/uAssets/master/filters/unbreak.txt", true),
)

/** Holds the native engine. Calls take a read lock; swapping the engine takes the write lock, so a freed engine is never in use. */
object AdBlocker {
    private const val TAG = "AdBlocker"
    private val lock = ReentrantReadWriteLock()
    private var handle = 0L
    private val worker = Executors.newSingleThreadExecutor { r -> Thread(r, "adblock-build").apply { isDaemon = true } }
    private val refreshing = AtomicBoolean(false)

    /** Bumped each time a new engine is installed; tabs re-register their document-start script when it changes. */
    val generation = AtomicInteger(0)

    @Volatile var ready = false
        private set

    val available: Boolean get() = Native.loaded

    /** Set by [installForTest]: the seeded test engine must not be replaced by the real lists. */
    @Volatile private var pinned = false

    @Volatile var onChange: (() -> Unit)? = null

    /** Build an engine from list texts and install it (blocking; call off the UI thread). */
    fun install(lists: List<Pair<String, Boolean>>, resourcesJson: String?): Boolean = install(lists, resourcesJson, false)

    private fun install(lists: List<Pair<String, Boolean>>, resourcesJson: String?, test: Boolean): Boolean {
        if (!Native.loaded || lists.isEmpty() || (pinned && !test)) return false
        val fresh = try {
            Native.create(lists.map { it.first }.toTypedArray(), lists.map { it.second }.toBooleanArray(), resourcesJson)
        } catch (e: Throwable) {
            Log.w(TAG, "engine build failed", e)
            0L
        }
        if (fresh == 0L) return false
        val old = lock.write {
            val o = handle
            handle = fresh
            ready = true
            generation.incrementAndGet()
            o
        }
        if (old != 0L) Native.free(old)
        onChange?.invoke()
        return true
    }

    fun shouldBlock(url: String, sourceUrl: String, type: String): Boolean {
        if (!ready) return false
        return lock.read { handle != 0L && Native.shouldBlock(handle, url, sourceUrl, type) }
    }

    fun cosmetic(url: String): String? {
        if (!ready) return null
        return lock.read { if (handle == 0L) null else Native.cosmetic(handle, url) }
    }

    private fun listsDir(ctx: Context) = File(ctx.filesDir, "filter-lists")

    /** Number of cached lists and the age of the oldest, for the settings screen. */
    fun cacheInfo(ctx: Context): Pair<Int, Long> {
        val files = FILTER_LISTS.map { File(listsDir(ctx), it.name) }.filter { it.exists() }
        return files.size to (files.minOfOrNull { it.lastModified() } ?: 0L)
    }

    /**
     * Background: build the engine from cached lists (so blocking starts quickly), then download
     * missing or stale lists (older than a day, or all when [force]) and rebuild if any changed.
     */
    fun start(ctx: Context, force: Boolean = false, download: Boolean = true) {
        val app = ctx.applicationContext
        if (!Native.loaded || pinned) return
        if (!refreshing.compareAndSet(false, true)) return
        worker.execute {
            try {
                if (!ready) loadCached(app)
                if (download && ListUpdater.update(listsDir(app), force) > 0) loadCached(app)
            } catch (e: Throwable) {
                Log.w(TAG, "refresh failed", e)
            } finally {
                refreshing.set(false)
            }
        }
    }

    private fun loadCached(ctx: Context) {
        val dir = listsDir(ctx)
        val lists = FILTER_LISTS.mapNotNull { l ->
            val f = File(dir, l.name)
            if (f.exists()) f.readText() to l.trusted else null
        }
        if (lists.isEmpty()) return
        val json = try {
            ctx.assets.open("ubo-scriptlets.json").bufferedReader().use { it.readText() }
        } catch (e: Exception) {
            Log.w(TAG, "scriptlet resources missing", e)
            null
        }
        install(lists, json)
    }

    /** Test hook: install lists given as text (the emulator test seeds a tiny list). */
    fun installForTest(ctx: Context, list: String, trusted: Boolean = false): Boolean {
        val json = ctx.assets.open("ubo-scriptlets.json").bufferedReader().use { it.readText() }
        pinned = true
        return install(listOf(list to trusted), json, true)
    }
}
