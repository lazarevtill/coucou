buildscript {
    dependencies {
        constraints {
            // AGP's build classpath still selects affected transitive releases.
            // These constraints update the tools only; none are added to the APK.
            for (module in listOf("bcprov-jdk18on", "bcpkix-jdk18on", "bcutil-jdk18on")) {
                add("classpath", "org.bouncycastle:$module:1.86") {
                    because("Bouncy Castle PKIX, ASN.1 and cryptographic security fixes")
                }
            }
            add("classpath", "com.google.guava:guava:33.7.2-jre") {
                because("CVE-2026-102554: bound collection deserialization allocations")
            }
            add("classpath", "org.apache.commons:commons-lang3:3.21.0") {
                because("CVE-2025-48924: bound recursive class-name processing")
            }
            add("classpath", "org.jdom:jdom2:2.0.6.1") {
                because("CVE-2021-33813: prevent XML external entity expansion")
            }
            add("classpath", "org.bitbucket.b_c:jose4j:0.9.7") {
                because("CVE-2024-29371: bound compressed JWE content")
            }
        }
    }
}

plugins {
    id("com.android.application") version "9.5.0" apply false
    id("org.jetbrains.kotlin.plugin.compose") version "2.4.21" apply false
}
