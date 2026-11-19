package com.marosmamrak.rfwatch.car

import android.text.SpannableString
import android.text.Spanned
import androidx.car.app.CarContext
import androidx.car.app.Screen
import androidx.car.app.model.Action
import androidx.car.app.model.CarColor
import androidx.car.app.model.CarIcon
import androidx.car.app.model.ForegroundCarColorSpan
import androidx.car.app.model.ItemList
import androidx.car.app.model.ListTemplate
import androidx.car.app.model.Row
import androidx.car.app.model.Template
import androidx.core.graphics.drawable.IconCompat
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.lifecycleScope
import androidx.lifecycle.repeatOnLifecycle
import com.marosmamrak.rfwatch.R
import com.marosmamrak.rfwatch.monitor.NodeRepository
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.launch

/** Live detections on the car screen; observes the same [NodeRepository] as the phone. */
class DetectionsScreen(carContext: CarContext) : Screen(carContext) {

    private var summary = CarSummary.from(NodeRepository.state.value)

    init {
        lifecycleScope.launch {
            repeatOnLifecycle(Lifecycle.State.STARTED) {
                NodeRepository.state
                    .map(CarSummary::from)
                    .distinctUntilChanged()
                    .collect { next ->
                        summary = next
                        invalidate()
                        // StateFlow conflates while we wait: at most one template per interval.
                        delay(REFRESH_INTERVAL_MS)
                    }
            }
        }
    }

    override fun onGetTemplate(): Template {
        val list = ItemList.Builder()
        for (row in summary.rows) {
            val builder = Row.Builder().setTitle(row.title)
            row.lines.forEachIndexed { i, line ->
                builder.addText(if (i == 0) colored(line, row.tone) else line)
            }
            icon(row.tone)?.let { builder.setImage(it) }
            list.addItem(builder.build())
        }
        @Suppress("DEPRECATION") // setTitle/setHeaderAction keep minCarApiLevel 1 working.
        return ListTemplate.Builder()
            .setTitle(summary.title)
            .setHeaderAction(Action.APP_ICON)
            .setSingleList(list.build())
            .build()
    }

    private fun colored(text: String, tone: CarSummary.Tone): CharSequence {
        val color = carColor(tone) ?: return text
        return SpannableString(text).apply {
            setSpan(ForegroundCarColorSpan.create(color), 0, length, Spanned.SPAN_INCLUSIVE_EXCLUSIVE)
        }
    }

    private fun carColor(tone: CarSummary.Tone): CarColor? = when (tone) {
        CarSummary.Tone.Alert -> CarColor.RED
        CarSummary.Tone.Clear -> CarColor.GREEN
        CarSummary.Tone.Neutral -> null
    }

    private fun icon(tone: CarSummary.Tone): CarIcon? {
        val res = when (tone) {
            CarSummary.Tone.Alert -> R.drawable.ic_alert
            CarSummary.Tone.Clear -> R.drawable.ic_check
            CarSummary.Tone.Neutral -> R.drawable.ic_signal
        }
        return CarIcon.Builder(IconCompat.createWithResource(carContext, res))
            .setTint(carColor(tone) ?: CarColor.DEFAULT)
            .build()
    }

    private companion object {
        const val REFRESH_INTERVAL_MS = 1_000L
    }
}
