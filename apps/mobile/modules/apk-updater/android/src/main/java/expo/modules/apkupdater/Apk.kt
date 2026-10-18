package expo.modules.apkupdater

import java.io.InputStream
import java.security.MessageDigest

/** Pure helpers. No Android classes, so a plain JVM test can run them. */
object Apk {
  /** "v1.2.3-beta" gives [1, 2, 3]. A part that is not a number gives null. */
  fun parse(version: String): List<Int>? {
    val core = version.trim().removePrefix("v").substringBefore('-').substringBefore('+')
    val parts = core.split('.').map { it.toIntOrNull() }
    return if (parts.any { it == null || it < 0 }) null else parts.filterNotNull()
  }

  /** True only when both versions parse and [candidate] is strictly higher. */
  fun isNewer(candidate: String, installed: String): Boolean {
    val a = parse(candidate) ?: return false
    val b = parse(installed) ?: return false
    for (i in 0 until maxOf(a.size, b.size)) {
      val x = a.getOrElse(i) { 0 }
      val y = b.getOrElse(i) { 0 }
      if (x != y) return x > y
    }
    return false
  }

  fun hex(bytes: ByteArray): String = bytes.joinToString("") { "%02x".format(it) }

  fun sha256(stream: InputStream): String {
    val digest = MessageDigest.getInstance("SHA-256")
    val buffer = ByteArray(64 * 1024)
    while (true) {
      val n = stream.read(buffer)
      if (n < 0) break
      digest.update(buffer, 0, n)
    }
    return hex(digest.digest())
  }
}
