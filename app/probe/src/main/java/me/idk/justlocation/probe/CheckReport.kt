package me.idk.justlocation.probe

/** Observations only; device scripts own the pass/fail rules. */
internal class CheckReport {
    private val lines = ArrayList<String>()

    fun section(title: String) {
        lines.add("")
        lines.add("## $title")
    }

    fun line(value: String) {
        lines.add(value)
    }

    fun render(): String = lines.joinToString("\n")
}
