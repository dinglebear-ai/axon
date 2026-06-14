# Add project-specific ProGuard rules here.
# Keep serialization metadata used by generated serializers without retaining
# all kotlinx.serialization library internals.
-keepattributes *Annotation*
-keepclassmembers class ** {
    @kotlinx.serialization.Serializable *;
}
-keep class com.axon.app.data.remote.**$$serializer { *; }
-keepclassmembers class com.axon.app.data.remote.** {
    public static ** Companion;
}
-keepclasseswithmembers class com.axon.app.data.remote.** {
    kotlinx.serialization.KSerializer serializer(...);
}

# Navigation Compose typed routes ask kotlinx.serialization for serializers at
# runtime. Keep the small route package intact so release/R8 builds can resolve
# data object and data class destinations.
-keep class com.axon.app.ui.nav.**Route { *; }
-keep class com.axon.app.ui.nav.**Route$$serializer { *; }
-keepclasseswithmembers class com.axon.app.ui.nav.**Route {
    kotlinx.serialization.KSerializer serializer(...);
}

# Tink references Error Prone annotations that are compile-time only. R8 reports
# them as missing in release minification unless they are explicitly ignored.
-dontwarn com.google.errorprone.annotations.CanIgnoreReturnValue
-dontwarn com.google.errorprone.annotations.CheckReturnValue
-dontwarn com.google.errorprone.annotations.Immutable
-dontwarn com.google.errorprone.annotations.RestrictedApi
