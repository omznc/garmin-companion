package expo.modules.apkupdater

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class ApkTest {
  @Test
  fun higherVersionIsNewer() {
    assertTrue(Apk.isNewer("0.5.0", "0.4.9"))
    assertTrue(Apk.isNewer("1.0.0", "0.99.99"))
    assertTrue(Apk.isNewer("0.10.0", "0.9.0"))
    assertTrue(Apk.isNewer("v0.1.1", "0.1"))
  }

  @Test
  fun sameOrLowerIsNotNewer() {
    assertFalse(Apk.isNewer("0.4.0", "0.4.0"))
    assertFalse(Apk.isNewer("0.4", "0.4.0"))
    assertFalse(Apk.isNewer("0.3.9", "0.4.0"))
  }

  @Test
  fun unparsableIsRefused() {
    assertNull(Apk.parse("abc"))
    assertNull(Apk.parse("1..2"))
    assertFalse(Apk.isNewer("", "0.1.0"))
    assertFalse(Apk.isNewer("9.9.9", "dev"))
  }

  @Test
  fun preReleaseSuffixIsIgnored() {
    assertEquals(listOf(1, 2, 3), Apk.parse("1.2.3-beta.1"))
  }

  @Test
  fun sha256MatchesKnownDigest() {
    assertEquals(
      "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
      Apk.sha256("abc".byteInputStream()),
    )
    assertEquals(
      "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
      Apk.sha256(ByteArray(0).inputStream()),
    )
  }
}
