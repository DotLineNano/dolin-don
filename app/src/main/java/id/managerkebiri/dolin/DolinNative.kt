package id.managerkebiri.dolin

object DolinNative {
    init {
        System.loadLibrary("dolin")
    }

    external fun archiveInfoReportJson(path: String): String
    external fun listArchiveReportJson(path: String): String
    external fun validateArchiveReportJson(path: String): String

    external fun extractArchiveEntryReportJson(
        archivePath: String,
        entryPath: String,
        outputFile: String
    ): String

    external fun extractArchiveReportJson(
        archivePath: String,
        outputDir: String
    ): String

    external fun packPathReportJson(
        inputPath: String,
        outputArchive: String
    ): String
}
