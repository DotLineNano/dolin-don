package id.managerkebiri.dolin

import kotlinx.serialization.builtins.ListSerializer

object DolinBridge {

    fun archiveInfo(path: String): DolinResult<ArchiveInfoReport> {
        val json = DolinNative.archiveInfoReportJson(path)
        return DolinJson.decode(json)
    }

    fun listEntries(path: String): DolinResult<List<ArchiveEntryReport>> {
        val json = DolinNative.listArchiveReportJson(path)
        return try {
            val err = DolinJson.parser.decodeFromString(DolinErrorResponse.serializer(), json)
            if (!err.error.isNullOrBlank()) {
                DolinResult.Error(err.error, json)
            } else {
                val value = DolinJson.parser.decodeFromString(
                    ListSerializer(ArchiveEntryReport.serializer()),
                    json
                )
                DolinResult.Success(value, json)
            }
        } catch (e: Throwable) {
            DolinResult.Error(e.message ?: "Unknown parse error", json)
        }
    }

    fun validate(path: String): DolinResult<ValidateArchiveReport> {
        val json = DolinNative.validateArchiveReportJson(path)
        return DolinJson.decode(json)
    }

    fun extractEntry(
        archivePath: String,
        entryPath: String,
        outputFile: String
    ): DolinResult<GenericOperationReport> {
        val json = DolinNative.extractArchiveEntryReportJson(archivePath, entryPath, outputFile)
        return DolinJson.decode(json)
    }

    fun extractAll(
        archivePath: String,
        outputDir: String
    ): DolinResult<GenericOperationReport> {
        val json = DolinNative.extractArchiveReportJson(archivePath, outputDir)
        return DolinJson.decode(json)
    }

    fun packPath(
        inputPath: String,
        outputArchive: String
    ): DolinResult<GenericOperationReport> {
        val json = DolinNative.packPathReportJson(inputPath, outputArchive)
        return DolinJson.decode(json)
    }

    fun rawArchiveInfo(path: String) = DolinNative.archiveInfoReportJson(path)
    fun rawList(path: String) = DolinNative.listArchiveReportJson(path)
    fun rawValidate(path: String) = DolinNative.validateArchiveReportJson(path)
    fun rawExtractEntry(archivePath: String, entryPath: String, outputFile: String) =
        DolinNative.extractArchiveEntryReportJson(archivePath, entryPath, outputFile)
    fun rawExtractAll(archivePath: String, outputDir: String) =
        DolinNative.extractArchiveReportJson(archivePath, outputDir)
    fun rawPackPath(inputPath: String, outputArchive: String) =
        DolinNative.packPathReportJson(inputPath, outputArchive)
}
