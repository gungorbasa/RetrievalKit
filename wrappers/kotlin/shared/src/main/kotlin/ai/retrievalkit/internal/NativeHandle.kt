package ai.retrievalkit.internal

import ai.retrievalkit.ClosedResourceException
import java.util.concurrent.atomic.AtomicLong

internal class NativeHandle(value: Long, private val releaser: (Long) -> Unit) : AutoCloseable {
    private val value = AtomicLong(value)

    fun requireOpen(owner: String): Long {
        val current = value.get()
        if (current == 0L) {
            throw ClosedResourceException("$owner is closed; create or load a new instance")
        }
        return current
    }

    override fun close() {
        val current = value.getAndSet(0)
        if (current != 0L) releaser(current)
    }
}
