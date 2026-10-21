package id.managerkebiri.dolin

import android.util.Log

object DolinSmokeTest {
    private const val TAG = "DolinSmokeTest"

    fun runAll(
        archivePath: String,
        sampleEntryPath: String,
        extractOneOutputFile: String,
        extractAllOutputDir: String,
        packInputPath: String,
        packOutputArchive: String
    ) {
        Log.d(TAG, "=== validate ===")
        logResult(DolinBridge.validate(archivePath))

        Log.d(TAG, "=== archive info ===")
        logResult(DolinBridge.archiveInfo(archivePath))

        Log.d(TAG, "=== list entries ===")
        logResult(DolinBridge.listEntries(archivePath))

        Log.d(TAG, "=== extract one ===")
        logResult(
            DolinBridge.extractEntry(
                archivePath = archivePath,
                entryPath = sampleEntryPath,
                outputFile = extractOneOutputFile
            )
        )

        Log.d(TAG, "=== extract all ===")
        logResult(
            DolinBridge.extractAll(
                archivePath = archivePath,
                outputDir = extractAllOutputDir
            )
        )

        Log.d(TAG, "=== pack path ===")
        logResult(
            DolinBridge.packPath(
                inputPath = packInputPath,
                outputArchive = packOutputArchive
            )
        )
    }

    private fun logResult(result: Any) {
        when (result) {
            is DolinResult.Success<*> -> {
                Log.d(TAG, "SUCCESS")
                Log.d(TAG, "VALUE: ${result.value}")
                Log.d(TAG, "RAW: ${result.rawJson}")
            }
            is DolinResult.Error -> {
                Log.e(TAG, "ERROR: ${result.message}")
                Log.e(TAG, "RAW: ${result.rawJson}")
            }
            else -> {
                Log.e(TAG, "UNKNOWN RESULT: $result")
            }
        }
    }
}
