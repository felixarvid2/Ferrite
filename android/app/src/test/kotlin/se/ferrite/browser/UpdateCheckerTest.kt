package se.ferrite.browser

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class UpdateCheckerTest {
    private val json = """[
      {"tag_name":"ferrite-v9.9.9","html_url":"https://github.com/o/r/releases/tag/ferrite-v9.9.9","assets":[]},
      {"tag_name":"ferrite-v0.3.0","draft":true,"html_url":"https://github.com/o/r/d"},
      {"tag_name":"ferrite-v0.2.0","html_url":"https://github.com/o/r/releases/tag/ferrite-v0.2.0",
       "assets":[{"name":"SHA256SUMS","browser_download_url":"https://github.com/o/r/sums"},
                 {"name":"Ferrite-0.2.0.apk","browser_download_url":"https://github.com/o/r/Ferrite-0.2.0.apk"}]},
      {"tag_name":"ferrite-android-v5.0.0","html_url":"https://github.com/o/r/x",
       "assets":[{"name":"Ferrite-5.0.0.apk","browser_download_url":"https://github.com/o/r/old.apk"}]},
      {"tag_name":"ferrite-v0.1.5","html_url":"https://github.com/o/r/releases/tag/x","assets":[]}
    ]"""

    @Test fun picksNewestReleaseThatHasAnApk() {
        val r = UpdateChecker.newest(json, "0.1.0")!!
        assertEquals("0.2.0", r.version)
        assertEquals("https://github.com/o/r/Ferrite-0.2.0.apk", r.url)
    }

    @Test fun nothingWhenCurrent() {
        assertNull(UpdateChecker.newest(json, "0.2.0"))
    }

    @Test fun dueRules() {
        assertTrue(UpdateChecker.isDue(0, 1000))
        assertFalse(UpdateChecker.isDue(1000, 2000))
        assertTrue(UpdateChecker.isDue(1000, 1000 + UpdateChecker.INTERVAL_MS))
        assertTrue(UpdateChecker.isDue(5000, 1000))
    }
}
