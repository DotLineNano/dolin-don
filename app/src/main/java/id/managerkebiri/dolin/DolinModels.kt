package id.managerkebiri.dolin

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.decodeFromString

@Serializable
data class DolinErrorResponse(
    val error: String? = null
)

@Serializable
data class GenericOperationReport(
    val ok: Boolean? = null,
    val message: String? = null,
    val path: String? = null,
    val archivePath: String? = null,
    val outputPath: String? = null,
    val outputDir: String? = null,
    val entryPath: String? = null,
    val entries: Int? = null,
    val files: Int? = null,
    val directories: Int? = null,
    val size: Long? = null,
    val valid: Boolean? = null
)

@Serializable
data class ArchiveEntryReport(
    val path: String? = null,
    val size: Long? = null,
    val compressedSize: Long? = null,
    val isDir: Boolean? = null,
    val method: String? = null,
    val crc32: String? = null
)

@Serializable
data class ArchiveInfoReport(
    val path: String? = null,
    val format: String? = null,
    val entries: Int? = null,
    val files: Int? = null,
    val directories: Int? = null,
    val totalSize: Long? = null,
    val comment: String? = null
)

@Serializable
data class ValidateArchiveReport(
    val path: String? = null,
    val valid: Boolean? = null,
    val message: String? = null,
    val format: String? = null
)

sealed class DolinResult<out T> {
    data class Success<T>(val value: T, val rawJson: String) : DolinResult<T>()
    data class Error(val message: String, val rawJson: String) : DolinResult<Nothing>()
}

object DolinJson {
    val parser = Json {
        ignoreUnknownKeys = true
        isLenient = true
        prettyPrint = false
        explicitNulls = false
    }

    inline fun <reified T> decode(json: String): DolinResult<T> {
        return try {
            val err = parser.decodeFromString<DolinErrorResponse>(json)
            if (!err.error.isNullOrBlank()) {
                DolinResult.Error(err.error, json)
            } else {
                val value = parser.decodeFromString<T>(json)
                DolinResult.Success(value, json)
            }
        } catch (_: Throwable) {
            try {
                val value = parser.decodeFromString<T>(json)
                DolinResult.Success(value, json)
            } catch (e: Throwable) {
                DolinResult.Error(e.message ?: "Unknown parse error", json)
            }
        }
    }

    fun decodeRaw(json: String): DolinResult<JsonElement> {
        return try {
            val err = parser.decodeFromString<DolinErrorResponse>(json)
            if (!err.error.isNullOrBlank()) {
                DolinResult.Error(err.error, json)
            } else {
                DolinResult.Success(parser.parseToJsonElement(json), json)
            }
        } catch (_: Throwable) {
            try {
                DolinResult.Success(parser.parseToJsonElement(json), json)
            } catch (e: Throwable) {
                DolinResult.Error(e.message ?: "Unknown parse error", json)
            }
        }
    }
}
