package se.ferrite.browser.adblock

/** JNI bindings to libferrite_android.so (android/rust). The library is missing in builds made without cargo-ndk; blocking is then off. */
object Native {
    val loaded: Boolean = try {
        System.loadLibrary("ferrite_android")
        true
    } catch (_: UnsatisfiedLinkError) {
        false
    }

    /** Builds an engine from filter list texts ([trusted] = uBO list, may use trusted scriptlets). 0 on failure. */
    @JvmStatic external fun create(lists: Array<String>, trusted: BooleanArray, resourcesJson: String?): Long

    @JvmStatic external fun free(handle: Long)

    @JvmStatic external fun shouldBlock(handle: Long, url: String, sourceUrl: String, requestType: String): Boolean

    /** The document-start script (scriptlets + cosmetic CSS, host-guarded) for a page, or null. */
    @JvmStatic external fun cosmetic(handle: Long, url: String): String?
}
