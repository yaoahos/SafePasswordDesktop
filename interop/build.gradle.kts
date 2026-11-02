plugins {
    kotlin("jvm") version "2.1.0"
    kotlin("plugin.serialization") version "2.1.0"
    application
}

group = "com.safeincloud"
version = "1.0.0"

kotlin {
    compilerOptions {
        jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17)
    }
}

java {
    sourceCompatibility = JavaVersion.VERSION_17
    targetCompatibility = JavaVersion.VERSION_17
}

sourceSets {
    main {
        kotlin {
            // 只读引用 Android 仓库的 core 源码（SafePassword 仓库不做任何修改）
            srcDir("../../SafePassword/core/src/main/kotlin")
            srcDir("src/main/kotlin")
        }
    }
}

dependencies {
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-core:1.9.0")
    implementation("org.jetbrains.kotlinx:kotlinx-serialization-json:1.7.3")
    implementation("io.ktor:ktor-client-core:3.0.3")
    implementation("io.ktor:ktor-client-auth:3.0.3")
}

application {
    mainClass.set("FixturesKt")
}
