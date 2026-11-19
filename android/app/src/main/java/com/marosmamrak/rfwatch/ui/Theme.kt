package com.marosmamrak.rfwatch.ui

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color

val AlertRed = Color(0xFFE53E3E)
val ClearGreen = Color(0xFF2F855A)
val UnknownGrey = Color(0xFF4A5568)

private val colors = darkColorScheme(
    primary = Color(0xFF4FD1C5),
    onPrimary = Color(0xFF0F1A24),
    secondary = Color(0xFFF6AD55),
    background = Color(0xFF0F1A24),
    surface = Color(0xFF0F1A24),
    surfaceVariant = Color(0xFF1A2733),
)

@Composable
fun RfWatchTheme(content: @Composable () -> Unit) {
    MaterialTheme(colorScheme = colors, content = content)
}
